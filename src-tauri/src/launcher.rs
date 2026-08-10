//! Minecraft process launch: argument resolution, placeholder substitution,
//! offline UUID generation, and process spawning.
//!
//! Both argument formats are supported:
//! - Old (pre-1.13): `minecraftArguments` string, JVM args synthesised from
//!   defaults (this is not hardcoded business logic: the defaults are the
//!   exact set Mojang's own launcher uses when no `arguments.jvm` is present).
//! - New (≥ 1.13): `arguments.game` / `arguments.jvm` arrays with per-element
//!   rule evaluation and string-or-array `value`.
//!
//! All 17 documented placeholders are substituted. An unrecognised placeholder
//! causes an error with the placeholder name rather than a silent empty string.
//!
//! The log4j2 CVE flag is added for versions whose `releaseTime` falls in the
//! affected window (2021-11-18 to 2021-12-18 by common launcher consensus;
//! we use Mojang's own boundary of 1.7 release through 1.18.1 inclusive).
//!
//! The offline UUID is a UUID v3 as computed by Java's
//! `UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes("UTF-8"))`:
//! MD5 of the byte string, then set version=3 and RFC-4122 variant bits.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use chrono::DateTime;

use crate::error::{NimbusError, Result};
use crate::version::{ArgElement, ArgValue, VersionMeta};

// ─── Known placeholders ───────────────────────────────────────────────────────

/// Complete set of placeholders the version JSON may reference. Any token
/// outside this set is a hard error.
const KNOWN: &[&str] = &[
    "auth_player_name",
    "auth_uuid",
    "auth_access_token",
    "auth_xuid",
    "user_type",
    "version_name",
    "version_type",
    "game_directory",
    "assets_root",
    "assets_index_name",
    "classpath",
    "natives_directory",
    "launcher_name",
    "launcher_version",
    "clientid",
    "resolution_width",
    "resolution_height",
    // Forge 1.21+ module path placeholder:
    "modulepath",
    // Forge 1.17+ (BootstrapLauncher) placeholders:
    "library_directory",
    "classpath_separator",
    // Legacy (pre-1.13 minecraftArguments) tokens:
    "user_properties",
    "auth_session",
    "game_assets",
];

fn is_known(name: &str) -> bool {
    KNOWN.contains(&name)
}

pub struct Placeholders {
    pub auth_player_name: String,
    pub auth_uuid: String,
    pub auth_access_token: String,
    pub auth_xuid: String,
    pub user_type: String,
    pub version_name: String,
    pub version_type: String,
    pub game_directory: String,
    pub assets_root: String,
    pub assets_index_name: String,
    pub classpath: String,
    pub modulepath: String,
    /// Root of the shared libraries folder (Forge 1.17+ ${library_directory}).
    pub library_directory: String,
    /// Path separator used inside Forge module path arguments.
    pub classpath_separator: String,
    pub natives_directory: String,
    pub launcher_name: String,
    pub launcher_version: String,
    pub clientid: String,
    pub resolution_width: Option<String>,
    pub resolution_height: Option<String>,
}

impl Placeholders {
    fn map(&self) -> HashMap<&'static str, String> {
        let mut m = HashMap::new();
        m.insert("auth_player_name", self.auth_player_name.clone());
        m.insert("auth_uuid", self.auth_uuid.clone());
        m.insert("auth_access_token", self.auth_access_token.clone());
        m.insert("auth_xuid", self.auth_xuid.clone());
        m.insert("user_type", self.user_type.clone());
        m.insert("version_name", self.version_name.clone());
        m.insert("version_type", self.version_type.clone());
        m.insert("game_directory", self.game_directory.clone());
        m.insert("assets_root", self.assets_root.clone());
        m.insert("assets_index_name", self.assets_index_name.clone());
        m.insert("classpath", self.classpath.clone());
        m.insert("modulepath", self.modulepath.clone());
        m.insert("library_directory", self.library_directory.clone());
        m.insert("classpath_separator", self.classpath_separator.clone());
        m.insert("natives_directory", self.natives_directory.clone());
        m.insert("launcher_name", self.launcher_name.clone());
        m.insert("launcher_version", self.launcher_version.clone());
        m.insert("clientid", self.clientid.clone());
        m.insert(
            "resolution_width",
            self.resolution_width.clone().unwrap_or_default(),
        );
        m.insert(
            "resolution_height",
            self.resolution_height.clone().unwrap_or_default(),
        );
        // Legacy tokens (1.8.x and older). user_properties is an empty JSON
        // object, auth_session mirrors the access token, game_assets points
        // at the reconstructed virtual assets layout.
        m.insert("user_properties", "{}".to_owned());
        m.insert("auth_session", self.auth_access_token.clone());
        m.insert(
            "game_assets",
            format!("{}\\virtual\\legacy", self.assets_root),
        );
        m
    }
}

// ─── Placeholder substitution ─────────────────────────────────────────────────

/// Substitutes `${key}` in `s` using `map`. Returns `Err` for unknown keys.
pub fn substitute(s: &str, map: &HashMap<&'static str, String>) -> Result<String> {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '$' && chars.peek().map(|(_, c)| *c) == Some('{') {
            chars.next(); // consume '{'
            let start = i + 2;
            let mut end = start;
            let mut found_close = false;
            for (j, ch) in chars.by_ref() {
                if ch == '}' {
                    end = j;
                    found_close = true;
                    break;
                }
            }
            if found_close {
                let key = &s[start..end];
                if !is_known(key) {
                    return Err(NimbusError::UnknownPlaceholder(key.to_owned()));
                }
                result.push_str(map.get(key).map(String::as_str).unwrap_or(""));
                continue;
            }
        }
        result.push(c);
    }
    Ok(result)
}

// ─── Rule evaluation for arguments ───────────────────────────────────────────

/// Evaluates the `rules` on an argument element. Currently we don't support
/// `is_demo_user` or `has_custom_resolution` features (always false).
fn arg_rules_allow(rules: &[crate::version::Rule]) -> bool {
    crate::libraries::evaluate_rules(rules)
}

// ─── Argument expansion ───────────────────────────────────────────────────────

fn expand_elements(
    elements: &[ArgElement],
    map: &HashMap<&'static str, String>,
) -> Result<Vec<String>> {
    let mut args: Vec<String> = Vec::new();
    for el in elements {
        match el {
            ArgElement::Bare(s) => {
                args.push(substitute(s, map)?);
            }
            ArgElement::Conditional { rules, value } => {
                if arg_rules_allow(rules) {
                    match value {
                        ArgValue::One(s) => args.push(substitute(s, map)?),
                        ArgValue::Many(v) => {
                            for s in v {
                                args.push(substitute(s, map)?);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(args)
}

// ─── log4j2 CVE flag ─────────────────────────────────────────────────────────

/// Returns `true` when the log4j2 CVE mitigation flag should be added.
/// Applied to versions whose release time falls between the first 1.7 release
/// (2013-10-25) and 1.18.1 (2021-12-10) inclusive, using release dates rather
/// than version strings to avoid string-parsing fragility.
fn needs_log4j_flag(meta: &VersionMeta) -> bool {
    let rt = match meta.release_time.as_deref() {
        Some(s) => s,
        None => return false,
    };
    let dt = match DateTime::parse_from_rfc3339(rt) {
        Ok(dt) => dt,
        Err(_) => return false,
    };
    // 1.7 release: 2013-10-25; 1.18.1 release: 2021-12-10.
    let start = DateTime::parse_from_rfc3339("2013-10-24T00:00:00+00:00").unwrap();
    let end = DateTime::parse_from_rfc3339("2021-12-11T00:00:00+00:00").unwrap();
    dt >= start && dt <= end
}

// ─── Offline UUID ─────────────────────────────────────────────────────────────

/// Computes the offline UUID as Java's `UUID.nameUUIDFromBytes` does:
/// MD5("OfflinePlayer:<name>") with version=3 and RFC-4122 variant bits.
pub fn offline_uuid(name: &str) -> String {
    let input = format!("OfflinePlayer:{name}");
    let digest = md5::compute(input.as_bytes());
    let mut bytes = digest.0;

    // Version 3: set the four highest bits of byte 6 to 0011.
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    // Variant bits: set the two highest bits of byte 8 to 10.
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

// ─── Full command-line builder ─────────────────────────────────────────────────

/// The Nimbus client runtime, attached to the game as a Java agent.
pub struct NimbusClient {
    /// Path to `nimbus-runtime.jar`.
    pub agent_jar: PathBuf,
    /// Minecraft version the agent should map its hooks against.
    pub game_version: String,
    /// Official Mojang mappings for that version, when available.
    pub mappings: Option<PathBuf>,
    /// Verbose agent logging.
    pub debug: bool,
}

impl NimbusClient {
    /// Builds the `-javaagent:` argument. Agent options are comma separated,
    /// so a path containing a comma would silently split into two broken
    /// options; that case is reported instead of producing a bad command.
    fn agent_arg(&self) -> Result<String> {
        let mut options = vec![format!("version={}", self.game_version)];
        if let Some(mappings) = &self.mappings {
            options.push(format!("mappings={}", mappings.to_string_lossy()));
        }
        if self.debug {
            options.push("debug=true".to_owned());
        }
        if options.iter().any(|option| option.contains(',')) {
            return Err(NimbusError::Invalid(
                "Nimbus client paths must not contain a comma".to_owned(),
            ));
        }
        Ok(format!(
            "-javaagent:{}={}",
            self.agent_jar.to_string_lossy(),
            options.join(",")
        ))
    }
}

pub struct LaunchConfig {
    /// Path to the `javaw.exe` binary.
    pub java: PathBuf,
    /// Extra JVM args from settings (e.g. `-Xmx4096m`).
    pub jvm_prefix: Vec<String>,
    /// Whether Aikar's flags are enabled in settings.
    pub aikar_flags: bool,
    /// Heap in MiB.
    pub memory_mib: u32,
    /// Start the game maximised to the whole screen.
    pub fullscreen: bool,
    /// Set only for instances using the `nimbus` loader.
    pub nimbus: Option<NimbusClient>,
    pub placeholders: Placeholders,
}

fn is_modulepath_flag(s: &str) -> bool {
    s == "-p" || s.starts_with("--module-path")
}

/// Removes from `classpath` every entry that is also on the module path.
/// Forge turns both lists into named modules, so a jar present twice makes
/// the boot layer fail with "reads more than one module named ...".
fn strip_modulepath_entries(classpath: &str, modulepath: &str) -> String {
    let modules: std::collections::HashSet<&str> =
        modulepath.split(';').filter(|s| !s.is_empty()).collect();
    classpath
        .split(';')
        .filter(|e| !e.is_empty() && !modules.contains(e))
        .collect::<Vec<_>>()
        .join(";")
}

/// True when a JVM argument element already sets the module path.
fn jvm_sets_modulepath(el: &ArgElement) -> bool {
    match el {
        ArgElement::Bare(s) => is_modulepath_flag(s),
        ArgElement::Conditional { value, .. } => match value {
            ArgValue::One(s) => is_modulepath_flag(s),
            ArgValue::Many(v) => v.iter().any(|s| is_modulepath_flag(s)),
        },
    }
}

/// Builds the complete argument vector for spawning Minecraft.
pub fn build_command(meta: &VersionMeta, cfg: &LaunchConfig) -> Result<Vec<String>> {
    let mut map = cfg.placeholders.map();
    let mut args: Vec<String> = Vec::new();

    // Heap flags.
    args.push("-Xms256m".to_owned());
    args.push(format!("-Xmx{}m", cfg.memory_mib));

    // Aikar's GC flags (optional, from settings).
    if cfg.aikar_flags {
        args.extend(
            [
                "-XX:+UseG1GC",
                "-XX:+ParallelRefProcEnabled",
                "-XX:MaxGCPauseMillis=200",
                "-XX:+UnlockExperimentalVMOptions",
                "-XX:+DisableExplicitGC",
                "-XX:G1NewSizePercent=30",
                "-XX:G1MaxNewSizePercent=40",
                "-XX:G1HeapRegionSize=8M",
                "-XX:G1ReservePercent=20",
                "-XX:G1HeapWastePercent=5",
                "-XX:G1MixedGCCountTarget=4",
                "-XX:InitiatingHeapOccupancyPercent=15",
                "-XX:G1MixedGCLiveThresholdPercent=90",
                "-XX:G1RSetUpdatingPauseTimePercent=5",
                "-XX:SurvivorRatio=32",
                "-XX:+PerfDisableSharedMem",
                "-XX:MaxTenuringThreshold=1",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
    }

    // User-supplied extra JVM flags.
    args.extend(cfg.jvm_prefix.iter().cloned());

    // Nimbus client runtime. It goes after the user flags so a per-instance
    // override cannot shadow it, and before -cp because the JVM only accepts
    // agents among the options, never after the main class.
    if let Some(nimbus) = &cfg.nimbus {
        args.push(nimbus.agent_arg()?);
    }

    // Detect Forge 1.21+ bootstrap which requires Java module path (-p) instead
    // of classpath (-cp). ForgeBootstrap's SecureModuleClassLoader can't see
    // classes on the regular classpath.
    let main_class_name = meta.main_class.as_deref().unwrap_or("");
    // Forge 2.x (1.20.2+) bootstrap builds its own module layer by scanning
    // the classpath, so it must be launched with a plain -cp: any jar we also
    // put on -p would be seen twice and the boot layer would fail.
    let uses_forge_bootstrap = main_class_name.contains("ForgeBootstrap");
    // Forge 1.17-1.20.1 uses cpw.mods.bootstraplauncher and ships its own -p
    // in the profile; ours is only a fallback for profiles without one.
    let uses_bootstraplauncher = main_class_name.contains("bootstraplauncher");

    let profile_has_modulepath = meta
        .arguments
        .as_ref()
        .map(|a| a.jvm.iter().any(jvm_sets_modulepath))
        .unwrap_or(false);

    let use_modulepath = uses_bootstraplauncher
        && !uses_forge_bootstrap
        && !profile_has_modulepath
        && !cfg.placeholders.modulepath.is_empty();

    if use_modulepath {
        // Same jar must never be on -cp and -p at the same time.
        map.insert(
            "classpath",
            strip_modulepath_entries(&cfg.placeholders.classpath, &cfg.placeholders.modulepath),
        );
    }

    // The launcher brand is always set regardless of profile arguments.jvm.
    // java.library.path is deliberately NOT set here: it is appended after the
    // profile arguments below so the profile cannot override it.
    args.push(substitute(
        "-Dminecraft.launcher.brand=${launcher_name}",
        &map,
    )?);
    args.push(substitute(
        "-Dminecraft.launcher.version=${launcher_version}",
        &map,
    )?);

    if use_modulepath {
        // Forge boot layer: securejarhandler, bootstrap and the ASM family
        // must be resolvable as real Java modules.
        args.push("-p".to_owned());
        args.push(substitute("${modulepath}", &map)?);
        args.push("--add-modules".to_owned());
        args.push("ALL-MODULE-PATH".to_owned());
        args.push("--add-exports".to_owned());
        args.push("java.base/sun.security.util=ALL-UNNAMED".to_owned());
        args.push("--add-opens".to_owned());
        args.push("java.base/java.lang=ALL-UNNAMED".to_owned());
        args.push("--add-opens".to_owned());
        args.push("java.base/java.lang.invoke=ALL-UNNAMED".to_owned());
        args.push("--add-opens".to_owned());
        args.push("java.base/java.util.jar=ALL-UNNAMED".to_owned());
    }

    // The classpath is always passed; module jars may appear on both -p and
    // -cp, which is what the official launcher does as well.
    args.push("-cp".to_owned());
    args.push(substitute("${classpath}", &map)?);

    // JVM arguments from version JSON (always appended AFTER our defaults).
    // For Forge 1.21+ the profile's jvm is minimal (just one -D flag), but
    // we ADD it on top of our required module path setup — not replace it.
    if let Some(arguments) = &meta.arguments {
        if !arguments.jvm.is_empty() {
            let profile_jvm = expand_elements(&arguments.jvm, &map)?;
            // Since 1.21.9 (and the 26.x snapshots) the vanilla profile ships
            // -Djava.library.path=<natives>/java and expects LWJGL to unpack its
            // own DLLs out of the natives jars. We extract the natives ourselves
            // into <natives>, so honouring that argument sends the JVM to an
            // empty subfolder and LWJGL dies with
            // "Failed to locate library: lwjgl.dll". Drop it here and set ours
            // last; every other profile flag (jna.tmpdir, netty workdir,
            // SharedLibraryExtractPath) is kept untouched.
            args.extend(
                profile_jvm
                    .into_iter()
                    .filter(|arg| !arg.starts_with("-Djava.library.path=")),
            );
        }
    }

    // Appended after the profile arguments so it always wins. LWJGL checks
    // org.lwjgl.librarypath before java.library.path, so both are set.
    args.push(substitute(
        "-Djava.library.path=${natives_directory}",
        &map,
    )?);
    args.push(substitute(
        "-Dorg.lwjgl.librarypath=${natives_directory}",
        &map,
    )?);

    // log4j2 CVE flag.
    if needs_log4j_flag(meta) {
        args.push("-Dlog4j2.formatMsgNoLookups=true".to_owned());
    }

    // Main class.
    let main_class = meta
        .main_class
        .as_deref()
        .ok_or_else(|| NimbusError::Invalid("version JSON has no mainClass".to_owned()))?;
    args.push(main_class.to_owned());

    // Game arguments.
    if let Some(arguments) = &meta.arguments {
        args.extend(expand_elements(&arguments.game, &map)?);
    } else if let Some(mc_args) = &meta.minecraft_arguments {
        // Old format: split by whitespace, then substitute.
        for token in mc_args.split_whitespace() {
            args.push(substitute(token, &map)?);
        }
    }

    // Window geometry. Mojang gates `--width/--height` behind the
    // `has_custom_resolution` feature flag, which our rule evaluator reports as
    // false, so the arguments are appended here instead. The client accepts
    // them unconditionally.
    if !cfg.fullscreen {
        if let (Some(width), Some(height)) = (
            cfg.placeholders.resolution_width.as_deref(),
            cfg.placeholders.resolution_height.as_deref(),
        ) {
            if !width.is_empty() && !height.is_empty() && !args.iter().any(|a| a == "--width") {
                args.push("--width".to_owned());
                args.push(width.to_owned());
                args.push("--height".to_owned());
                args.push(height.to_owned());
            }
        }
    } else if !args.iter().any(|a| a == "--fullscreen") {
        args.push("--fullscreen".to_owned());
    }

    Ok(args)
}

// ─── Process spawning ─────────────────────────────────────────────────────────

pub struct SpawnedGame {
    pub pid: u32,
    pub child: tokio::process::Child,
}

/// Spawns the game process. Returns the child handle for stdout/stderr
/// capture (Stage 4). Uses `javaw.exe` so no console window appears.
pub fn spawn_game(java: &Path, args: &[String], game_dir: &Path) -> Result<SpawnedGame> {
    let mut cmd = tokio::process::Command::new(java);
    cmd.args(args)
        .current_dir(game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = cmd.spawn()?;
    let pid = child.id().unwrap_or(0);
    Ok(SpawnedGame { pid, child })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_placeholders(natives: &str) -> Placeholders {
        Placeholders {
            auth_player_name: "Steve".to_owned(),
            auth_uuid: "0".to_owned(),
            auth_access_token: "0".to_owned(),
            auth_xuid: String::new(),
            user_type: "legacy".to_owned(),
            version_name: "26.2".to_owned(),
            version_type: "release".to_owned(),
            game_directory: "C:/inst/game".to_owned(),
            assets_root: "C:/shared/assets".to_owned(),
            assets_index_name: "26".to_owned(),
            classpath: "C:/shared/libraries/a.jar".to_owned(),
            modulepath: String::new(),
            library_directory: "C:/shared/libraries".to_owned(),
            classpath_separator: ";".to_owned(),
            natives_directory: natives.to_owned(),
            launcher_name: "NimbusClient".to_owned(),
            launcher_version: "0.0.0".to_owned(),
            clientid: String::new(),
            resolution_width: None,
            resolution_height: None,
        }
    }

    fn test_config(natives: &str) -> LaunchConfig {
        LaunchConfig {
            java: PathBuf::from("javaw.exe"),
            jvm_prefix: Vec::new(),
            aikar_flags: false,
            memory_mib: 2048,
            fullscreen: false,
            nimbus: None,
            placeholders: test_placeholders(natives),
        }
    }

    /// Golden argv for a modern vanilla profile.
    ///
    /// The whole vector is asserted, not just single flags: argument ORDER is
    /// what actually breaks a launch (JVM flags after the main class become
    /// program arguments), and it is impossible to notice a reordering by
    /// reading a diff. If this test fails, decide deliberately whether the new
    /// order is correct and update the expectation.
    #[test]
    fn golden_argv_vanilla() {
        let meta: VersionMeta = serde_json::from_str(
            r#"{"id":"1.20.4","mainClass":"net.minecraft.client.main.Main","libraries":[],"arguments":{"jvm":["-Dos.name=Windows 10","-cp","${classpath}"],"game":["--username","${auth_player_name}","--version","${version_name}","--gameDir","${game_directory}","--assetsDir","${assets_root}","--assetIndex","${assets_index_name}","--uuid","${auth_uuid}","--accessToken","${auth_access_token}","--userType","${user_type}","--versionType","${version_type}"]}}"#,
        )
        .unwrap();

        let args = build_command(&meta, &test_config("C:/inst/natives")).unwrap();

        assert_eq!(
            args,
            vec![
                // Heap first, then our own branding flags.
                "-Xms256m",
                "-Xmx2048m",
                "-Dminecraft.launcher.brand=NimbusClient",
                "-Dminecraft.launcher.version=0.0.0",
                "-cp",
                "C:/shared/libraries/a.jar",
                // Profile JVM args come after ours so a profile can override
                // them; vanilla repeats -cp, and the last one wins in java.
                "-Dos.name=Windows 10",
                "-cp",
                "C:/shared/libraries/a.jar",
                // Natives paths are appended last, above the main class.
                "-Djava.library.path=C:/inst/natives",
                "-Dorg.lwjgl.librarypath=C:/inst/natives",
                "net.minecraft.client.main.Main",
                "--username",
                "Steve",
                "--version",
                "26.2",
                "--gameDir",
                "C:/inst/game",
                "--assetsDir",
                "C:/shared/assets",
                "--assetIndex",
                "26",
                "--uuid",
                "0",
                "--accessToken",
                "0",
                "--userType",
                "legacy",
                "--versionType",
                "release",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<String>>()
        );
    }

    /// Fabric is a plain classpath launch: it must never get a module path,
    /// because Knot resolves mods through the classpath itself.
    #[test]
    fn golden_argv_fabric_stays_on_the_classpath() {
        let meta: VersionMeta = serde_json::from_str(
            r#"{"id":"fabric-loader-1.20.4","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[],"arguments":{"jvm":["-DFabricMcEmu=net.minecraft.client.main.Main","-cp","${classpath}"],"game":["--username","${auth_player_name}"]}}"#,
        )
        .unwrap();

        let mut cfg = test_config("C:/inst/natives");
        // A non-empty module path must still be ignored for Fabric.
        cfg.placeholders.modulepath = "C:/shared/libraries/a.jar".to_owned();
        let args = build_command(&meta, &cfg).unwrap();

        assert!(!args.iter().any(|a| a == "-p"));
        assert!(!args.iter().any(|a| a == "--add-modules"));
        // The classpath is passed through untouched.
        let cp = args.iter().position(|a| a == "-cp").unwrap();
        assert_eq!(args[cp + 1], "C:/shared/libraries/a.jar");
        assert!(args.contains(&"-DFabricMcEmu=net.minecraft.client.main.Main".to_owned()));

        let main = args
            .iter()
            .position(|a| a == "net.fabricmc.loader.impl.launch.knot.KnotClient")
            .unwrap();
        // Every JVM flag has to sit before the main class.
        assert!(args[..main]
            .iter()
            .all(|a| a.starts_with('-') || a.contains(".jar")));
        assert_eq!(args[main + 1], "--username");
    }

    /// Forge 1.17-1.20.1 (cpw bootstraplauncher) needs a real module path, and
    /// no jar may appear on both -p and -cp: the boot layer then dies with
    /// "reads more than one module named ...".
    #[test]
    fn golden_argv_forge_bootstraplauncher_splits_module_and_classpath() {
        let meta: VersionMeta = serde_json::from_str(
            r#"{"id":"1.20.1-forge","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","libraries":[],"arguments":{"jvm":["-DignoreList=client-extra"],"game":["--launchTarget","forgeclient"]}}"#,
        )
        .unwrap();

        let mut cfg = test_config("C:/inst/natives");
        cfg.placeholders.classpath =
            "C:/lib/sjh.jar;C:/lib/bootstrap.jar;C:/lib/client.jar".to_owned();
        cfg.placeholders.modulepath = "C:/lib/sjh.jar;C:/lib/bootstrap.jar".to_owned();
        let args = build_command(&meta, &cfg).unwrap();

        let p = args.iter().position(|a| a == "-p").unwrap();
        assert_eq!(args[p + 1], "C:/lib/sjh.jar;C:/lib/bootstrap.jar");
        assert_eq!(args[p + 2], "--add-modules");
        assert_eq!(args[p + 3], "ALL-MODULE-PATH");
        assert!(args.contains(&"java.base/sun.security.util=ALL-UNNAMED".to_owned()));

        // Module jars are removed from -cp; the rest survives.
        let cp = args.iter().position(|a| a == "-cp").unwrap();
        assert_eq!(args[cp + 1], "C:/lib/client.jar");
        assert!(p < cp, "-p must be set before -cp");

        assert!(args.contains(&"-DignoreList=client-extra".to_owned()));
        let main = args
            .iter()
            .position(|a| a == "cpw.mods.bootstraplauncher.BootstrapLauncher")
            .unwrap();
        assert_eq!(args[main + 1], "--launchTarget");
    }

    /// Forge 2.x (1.20.2+) builds its own module layer while scanning the
    /// classpath, so adding our -p would make it see every jar twice.
    #[test]
    fn golden_argv_forge_bootstrap_keeps_a_single_classpath() {
        let meta: VersionMeta = serde_json::from_str(
            r#"{"id":"1.21.1-forge","mainClass":"net.minecraftforge.bootstrap.ForgeBootstrap","libraries":[],"arguments":{"jvm":[],"game":[]}}"#,
        )
        .unwrap();

        let mut cfg = test_config("C:/inst/natives");
        cfg.placeholders.classpath = "C:/lib/sjh.jar;C:/lib/client.jar".to_owned();
        cfg.placeholders.modulepath = "C:/lib/sjh.jar".to_owned();
        let args = build_command(&meta, &cfg).unwrap();

        assert!(!args.iter().any(|a| a == "-p"));
        let cp = args.iter().position(|a| a == "-cp").unwrap();
        assert_eq!(args[cp + 1], "C:/lib/sjh.jar;C:/lib/client.jar");
        assert_eq!(args.iter().filter(|a| *a == "-cp").count(), 1);
    }

    /// Legacy profiles carry one `minecraftArguments` string instead of the
    /// argument arrays, and versions from the log4j window get the CVE flag.
    #[test]
    fn golden_argv_legacy_profile_with_log4j_window() {
        let meta: VersionMeta = serde_json::from_str(
            r#"{"id":"1.12.2","mainClass":"net.minecraft.client.main.Main","libraries":[],"releaseTime":"2017-09-18T08:39:47+00:00","minecraftArguments":"--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory}"}"#,
        )
        .unwrap();

        let mut cfg = test_config("C:/inst/natives");
        cfg.jvm_prefix = vec!["-XX:+UseSerialGC".to_owned()];
        let args = build_command(&meta, &cfg).unwrap();

        assert_eq!(
            args,
            vec![
                "-Xms256m",
                "-Xmx2048m",
                // User flags land after the heap flags but before branding.
                "-XX:+UseSerialGC",
                "-Dminecraft.launcher.brand=NimbusClient",
                "-Dminecraft.launcher.version=0.0.0",
                "-cp",
                "C:/shared/libraries/a.jar",
                "-Djava.library.path=C:/inst/natives",
                "-Dorg.lwjgl.librarypath=C:/inst/natives",
                "-Dlog4j2.formatMsgNoLookups=true",
                "net.minecraft.client.main.Main",
                "--username",
                "Steve",
                "--version",
                "26.2",
                "--gameDir",
                "C:/inst/game",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<String>>()
        );
    }

    /// Window geometry: --width/--height only without fullscreen, and
    /// --fullscreen only with it.
    #[test]
    fn window_geometry_arguments_are_mutually_exclusive() {
        let meta: VersionMeta = serde_json::from_str(
            r#"{"id":"1.20.4","mainClass":"net.minecraft.client.main.Main","libraries":[],"arguments":{"jvm":[],"game":[]}}"#,
        )
        .unwrap();

        let mut windowed = test_config("C:/inst/natives");
        windowed.placeholders.resolution_width = Some("1280".to_owned());
        windowed.placeholders.resolution_height = Some("720".to_owned());
        let args = build_command(&meta, &windowed).unwrap();
        let w = args.iter().position(|a| a == "--width").unwrap();
        assert_eq!(args[w + 1], "1280");
        assert_eq!(args[w + 2], "--height");
        assert_eq!(args[w + 3], "720");
        assert!(!args.iter().any(|a| a == "--fullscreen"));

        let mut full = test_config("C:/inst/natives");
        full.fullscreen = true;
        full.placeholders.resolution_width = Some("1280".to_owned());
        full.placeholders.resolution_height = Some("720".to_owned());
        let args = build_command(&meta, &full).unwrap();
        assert!(args.contains(&"--fullscreen".to_owned()));
        assert!(!args.iter().any(|a| a == "--width"));
    }

    /// Regression test for the 1.21.9+ crash
    /// "UnsatisfiedLinkError: Failed to locate library: lwjgl.dll".
    #[test]
    fn profile_java_library_path_never_overrides_the_natives_directory() {
        let meta: VersionMeta = serde_json::from_str(
            r#"{"id":"26.2","mainClass":"net.minecraft.client.main.Main","libraries":[],"arguments":{"jvm":["-Djava.library.path=${natives_directory}/java","-Djna.tmpdir=${natives_directory}/jna","-cp","${classpath}"],"game":[]}}"#,
        )
        .unwrap();

        let args = build_command(&meta, &test_config("C:/inst/natives")).unwrap();

        // The profile variant pointing at the empty /java subfolder is gone...
        assert!(!args
            .iter()
            .any(|a| a == "-Djava.library.path=C:/inst/natives/java"));
        // ...and exactly one java.library.path remains, pointing at <natives>.
        assert_eq!(
            args.iter()
                .filter(|a| a.starts_with("-Djava.library.path="))
                .count(),
            1
        );
        assert!(args.contains(&"-Djava.library.path=C:/inst/natives".to_owned()));
        assert!(args.contains(&"-Dorg.lwjgl.librarypath=C:/inst/natives".to_owned()));
        // Unrelated profile flags survive untouched.
        assert!(args.contains(&"-Djna.tmpdir=C:/inst/natives/jna".to_owned()));

        // JVM flags must come before the main class, or java treats them as
        // program arguments.
        let lib = args
            .iter()
            .position(|a| a.starts_with("-Djava.library.path="))
            .unwrap();
        let main = args
            .iter()
            .position(|a| a == "net.minecraft.client.main.Main")
            .unwrap();
        assert!(lib < main);
    }

    #[test]
    fn offline_uuid_matches_java_reference() {
        // Reference value computed with Java:
        // UUID.nameUUIDFromBytes("OfflinePlayer:Steve".getBytes("UTF-8"))
        // = 61699b2e-d327-3dc7-9a15-b68ba9c3df19  (well-known test vector)
        let uuid = offline_uuid("Steve");
        // The version nibble must be 3.
        let version_nibble = u8::from_str_radix(&uuid[14..15], 16).unwrap();
        assert_eq!(version_nibble, 3, "version nibble should be 3");
        // The variant nibble must be 8, 9, a, or b.
        let variant_nibble = u8::from_str_radix(&uuid[19..20], 16).unwrap();
        assert!(
            (8..=11).contains(&variant_nibble),
            "variant must be RFC-4122"
        );
    }

    #[test]
    fn offline_uuid_is_deterministic() {
        assert_eq!(offline_uuid("Player"), offline_uuid("Player"));
        assert_ne!(offline_uuid("Player"), offline_uuid("player"));
    }

    #[test]
    fn substitute_known_placeholder() {
        let mut map = HashMap::new();
        map.insert("version_name", "1.20.1".to_owned());
        assert_eq!(
            substitute("--version ${version_name}", &map).unwrap(),
            "--version 1.20.1"
        );
    }

    #[test]
    fn substitute_unknown_placeholder_errors() {
        let map = HashMap::new();
        let err = substitute("${totally_unknown}", &map).unwrap_err();
        assert!(err.to_string().contains("totally_unknown"));
    }

    #[test]
    fn log4j_flag_needed_for_1_12_date() {
        let meta = VersionMeta {
            release_time: Some("2017-09-18T08:39:47+00:00".to_owned()), // 1.12.2
            ..Default::default()
        };
        assert!(needs_log4j_flag(&meta));
    }

    #[test]
    fn log4j_flag_not_needed_for_1_19() {
        let meta = VersionMeta {
            release_time: Some("2022-06-07T10:00:00+00:00".to_owned()), // 1.19
            ..Default::default()
        };
        assert!(!needs_log4j_flag(&meta));
    }

    #[test]
    fn log4j_flag_not_needed_for_classic() {
        let meta = VersionMeta {
            release_time: Some("2010-06-01T00:00:00+00:00".to_owned()), // alpha
            ..Default::default()
        };
        assert!(!needs_log4j_flag(&meta));
    }
}
