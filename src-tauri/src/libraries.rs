//! Library filtering, classpath construction, and native artifact resolution.
//!
//! Rules evaluation: absent rules → allow. Present rules → default deny; each
//! rule is tested in order against the current OS; last matching rule wins.
//!
//! Two natives formats are supported:
//! - Modern: `downloads.artifact` whose `path` contains `natives-windows`.
//! - Legacy: `natives.windows` key → `downloads.classifiers[key]`, with
//!   `${arch}` substituted to `64` on x64 Windows.
//!
//! Classpath deduplication: after filtering, entries with the same
//! `groupId:artifactId[:classifier]` keep the one with the highest Maven
//! version string. The classifier is part of the key on purpose: Forge ships
//! `forge:<ver>:client`, `forge:<ver>:universal` and
//! `net.minecraft:client:<ver>:extra` / `:srg` as separate jars that all have
//! to stay on the classpath.

use std::path::Path;

use crate::version::{LibraryJson, Rule};

/// A resolved, downloadable library artifact.
#[derive(Debug, Clone)]
pub struct ResolvedLib {
    /// Maven coordinate, used for deduplication.
    pub name: String,
    /// Path inside the shared libraries directory.
    pub rel_path: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
    /// True when this entry should be extracted to `natives/` rather than
    /// placed on the classpath.
    pub is_native: bool,
    /// Exclusion prefixes for zip extraction (e.g. `META-INF/`).
    pub extract_exclude: Vec<String>,
}

// ─── OS helpers ──────────────────────────────────────────────────────────────

fn current_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn current_arch() -> &'static str {
    // Tauri only targets x64/arm64 Windows; we report x64 for the `${arch}`
    // substitution used by legacy natives.
    if cfg!(target_arch = "x86") {
        "32"
    } else {
        "64"
    }
}

/// Returns the current OS version string in the form Mojang's version.json
/// rules expect to match against (e.g. `10.0`). Reads
/// `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion` on Windows; returns an
/// empty string on other platforms (rules with a `version` condition simply
/// won't match there, which is conservative and safe).
fn current_os_version() -> String {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(key) = hklm.open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion") {
            let major: Result<u32, _> = key.get_value("CurrentMajorVersionNumber");
            let minor: Result<u32, _> = key.get_value("CurrentMinorVersionNumber");
            if let (Ok(major), Ok(minor)) = (major, minor) {
                return format!("{major}.{minor}");
            }
            if let Ok(version) = key.get_value::<String, _>("CurrentVersion") {
                return version;
            }
        }
        String::new()
    }
    #[cfg(not(windows))]
    {
        String::new()
    }
}

// ─── Rule evaluation ──────────────────────────────────────────────────────

/// Mojang-style CPU architecture name, as used by `os.arch` rules in
/// version.json (`x86`, `x86_64`, `arm64`).
fn current_os_arch() -> &'static str {
    if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    }
}

/// OS token used inside native classifiers. Note this is `macos`, not the
/// `osx` spelling used by rules.
fn current_native_os_token() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Native classifier that targets the running platform, e.g. `windows` on x64
/// Windows, `windows-arm64` on ARM Windows, `macos-arm64` on Apple silicon.
fn current_native_classifier() -> String {
    let os = current_native_os_token();
    match current_os_arch() {
        "x86_64" => os.to_owned(),
        arch => format!("{os}-{arch}"),
    }
}

/// Returns `true` when a `natives-*` artifact belongs to the running platform.
///
/// Since Minecraft 1.21.9 the manifests gate `natives-windows`,
/// `natives-windows-arm64` and `natives-windows-x86` by `os.name` alone, with
/// no arch condition, so all three passed rule evaluation on x64 and were
/// downloaded and extracted on top of each other.
fn native_classifier_matches_current(rel_path: &str) -> bool {
    let lower = rel_path.to_ascii_lowercase();
    let Some(idx) = lower.rfind("natives-") else {
        return true;
    };
    let tail = &lower[idx + "natives-".len()..];
    let tail = tail.split('.').next().unwrap_or(tail);
    tail == current_native_classifier().as_str()
}

/// Returns whether the rule's OS condition matches the running OS.
fn os_matches(rule: &Rule) -> bool {
    let Some(os_cond) = &rule.os else {
        return true; // No OS condition → always matches.
    };
    if let Some(name) = &os_cond.name {
        if name != current_os_name() {
            return false;
        }
    }
    if let Some(version_re) = &os_cond.version {
        // os.version is a regex matched against the full OS version string,
        // exactly like Mojang's own launcher (e.g. an anchored "10." prefix).
        let os_ver = current_os_version();
        match regex::Regex::new(version_re) {
            Ok(re) => {
                if !re.is_match(&os_ver) {
                    return false;
                }
            }
            Err(_) => {
                // Malformed regex in the manifest: be conservative and treat
                // the rule as non-matching rather than panicking.
                return false;
            }
        }
    }
    if let Some(arch) = &os_cond.arch {
        // Mojang writes "x86", "x86_64" or "arm64" here. Comparing that against
        // a bitness string ("32"/"64") made every arm64-only rule match on x64.
        if arch.as_str() != current_os_arch() {
            return false;
        }
    }
    true
}

fn rule_matches(rule: &Rule) -> bool {
    // All launcher features (is_demo_user, has_custom_resolution, the
    // is_quick_play_* family, …) are false in our context. Mojang feature
    // rules always require a feature to be true, so any rule carrying a
    // features condition can never match — including future unknown ones.
    if rule.features.is_some() {
        return false;
    }
    os_matches(rule)
}

/// Evaluates the `rules` array. Returns `true` when the library should be
/// included. An empty or absent rules list always returns `true`.
pub fn evaluate_rules(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allow = false;
    for rule in rules {
        if rule_matches(rule) {
            allow = rule.action == "allow";
        }
    }
    allow
}

// ─── Native classifier key ────────────────────────────────────────────────────

/// Resolves the native classifier key for Windows, substituting `${arch}`.
fn native_key(lib: &LibraryJson) -> Option<String> {
    let natives = lib.natives.as_ref()?;
    let raw = natives.windows.as_deref()?;
    Some(raw.replace("${arch}", current_arch()))
}

// ─── Maven coordinate parsing ─────────────────────────────────────────────────

/// A parsed Maven coordinate:
/// `group:artifact:version[:classifier][@extension]`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Coord<'a> {
    group: &'a str,
    artifact: &'a str,
    version: &'a str,
    classifier: Option<&'a str>,
    extension: &'a str,
}

/// Parses a full Maven coordinate. Returns `None` when the string has fewer
/// than three colon-separated segments (not a coordinate we can lay out).
///
/// Forge install profiles use the extended form heavily, e.g.
/// `net.minecraft:client:1.21:mappings@tsrg` →
/// `net/minecraft/client/1.21/client-1.21-mappings.tsrg`.
/// Splitting on the first three colons only (the naive approach) leaves
/// `1.21:mappings@tsrg` as the "version", which is an illegal Windows folder
/// name and makes the installer processors fail with
/// "Could not make output folders".
fn parse_coord(name: &str) -> Option<Coord<'_>> {
    let (coords, extension) = match name.split_once('@') {
        Some((c, e)) if !e.is_empty() => (c, e),
        _ => (name, "jar"),
    };
    let mut parts = coords.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    let classifier = parts.next().filter(|c| !c.is_empty());
    Some(Coord {
        group,
        artifact,
        version,
        classifier,
        extension,
    })
}

/// Deduplication key: `group:artifact` plus the classifier when present.
///
/// The classifier MUST be part of the key. Forge 1.13+ profiles list several
/// artifacts that share `group:artifact:version` and differ only by
/// classifier — `net.minecraftforge:forge:<ver>:client` (the patched client
/// produced by the installer processors) and `:universal`, plus
/// `net.minecraft:client:<ver>:extra` and `:srg`. Keying on `group:artifact`
/// alone silently dropped the patched client jar, and FML then died with
/// "Could not find net/minecraft/client/Minecraft.class in classloader".
fn coord_key(name: &str) -> String {
    match parse_coord(name) {
        Some(coord) => match coord.classifier {
            Some(classifier) => {
                format!("{}:{}:{}", coord.group, coord.artifact, classifier)
            }
            None => format!("{}:{}", coord.group, coord.artifact),
        },
        None => name.split('@').next().unwrap_or(name).to_owned(),
    }
}

/// Returns the version segment of a coordinate, ignoring classifier and
/// extension so that comparisons see `1.21` and not `1.21:mappings@tsrg`.
fn coord_version(name: &str) -> &str {
    let coords = name.split('@').next().unwrap_or(name);
    let mut colons = coords.match_indices(':').map(|(i, _)| i);
    let (Some(_first), Some(second)) = (colons.next(), colons.next()) else {
        return "";
    };
    let rest = &coords[second + 1..];
    rest.split(':').next().unwrap_or(rest)
}

/// Compares two Maven version strings numerically by splitting on `.` and
/// comparing each component as an integer. Falls back to lexicographic
/// comparison for non-numeric segments.
fn version_gt(a: &str, b: &str) -> bool {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();
    let len = a_parts.len().max(b_parts.len());
    for i in 0..len {
        let a_seg = a_parts.get(i).copied().unwrap_or("0");
        let b_seg = b_parts.get(i).copied().unwrap_or("0");
        match (a_seg.parse::<u64>(), b_seg.parse::<u64>()) {
            (Ok(an), Ok(bn)) => {
                if an != bn {
                    return an > bn;
                }
            }
            _ => {
                if a_seg != b_seg {
                    return a_seg > b_seg;
                }
            }
        }
    }
    false
}

// ─── Path derivation ─────────────────────────────────────────────────────────

/// Derives the standard Maven layout path for a coordinate, honouring the
/// optional `:classifier` and `@extension` suffixes.
///
/// - `com.google.guava:guava:21.0`
///   → `com/google/guava/guava/21.0/guava-21.0.jar`
/// - `net.minecraft:client:1.21:mappings@tsrg`
///   → `net/minecraft/client/1.21/client-1.21-mappings.tsrg`
pub fn maven_path(name: &str) -> String {
    let Some(coord) = parse_coord(name) else {
        return format!("{}.jar", name.replace([':', '@'], "/"));
    };
    let group = coord.group.replace('.', "/");
    let artifact = coord.artifact;
    let version = coord.version;
    let ext = coord.extension;
    let file = match coord.classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    format!("{group}/{artifact}/{version}/{file}")
}

// ─── Main filtering function ──────────────────────────────────────────────────

/// Filters a version JSON library list for the current OS and returns resolved
/// entries ready for download and classpath construction.
pub fn resolve_libraries(
    libs: &[LibraryJson],
    libraries_root: &Path,
) -> Vec<ResolvedLib> {
    let mut resolved: Vec<ResolvedLib> = Vec::new();

    for lib in libs {
        let rules = lib.rules.as_deref().unwrap_or(&[]);
        if !evaluate_rules(rules) {
            continue;
        }

        let downloads = lib.downloads.as_ref();

        // ── Native artifact ───────────────────────────────────────────────
        if let Some(key) = native_key(lib) {
            // Try modern classifiers first.
            let classifier = downloads
                .and_then(|d| d.classifiers.as_ref())
                .and_then(|c| c.entries.get(&key));

            if let Some(cls) = classifier {
                let rel = cls
                    .path
                    .clone()
                    .unwrap_or_else(|| maven_path_with_classifier(&lib.name, &key));
                resolved.push(ResolvedLib {
                    name: lib.name.clone(),
                    rel_path: rel,
                    url: cls.url.clone(),
                    sha1: cls.sha1.clone(),
                    size: cls.size,
                    is_native: true,
                    extract_exclude: lib
                        .extract
                        .as_ref()
                        .map(|e| e.exclude.clone())
                        .unwrap_or_default(),
                });
            }
        }

        // ── Regular artifact ──────────────────────────────────────────────
        // Check if this is purely a native-only library (no artifact entry)
        // or a dual-purpose lib that also goes on the classpath.
        let artifact = downloads.and_then(|d| d.artifact.as_ref());

        if let Some(art) = artifact {
            // Skip if it was already pushed above via a legacy classifier.
            let rel = art
                .path
                .clone()
                .unwrap_or_else(|| maven_path(&lib.name));
            let is_natives_path = rel.contains("natives-");

            // A natives jar for another OS or architecture must not be
            // downloaded, extracted, or placed on the classpath.
            if is_natives_path && !native_classifier_matches_current(&rel) {
                continue;
            }

            if !is_natives_path || native_key(lib).is_none() {
                // Modern natives format: the artifact itself is the natives
                // jar and shares its coordinate with the core artifact, so it
                // must be flagged native to survive classpath dedup and get
                // extracted into the natives directory.
                let is_native = is_natives_path;
                resolved.push(ResolvedLib {
                    name: lib.name.clone(),
                    rel_path: rel,
                    url: art.url.clone(),
                    sha1: art.sha1.clone(),
                    size: art.size,
                    is_native,
                    extract_exclude: if is_native {
                        lib.extract
                            .as_ref()
                            .map(|e| e.exclude.clone())
                            .unwrap_or_default()
                    } else {
                        vec![]
                    },
                });
            }
        } else if native_key(lib).is_none() {
            // No downloads block at all — derive path from name and any URL
            // provided at the library level (some older format entries do this).
            let rel = maven_path(&lib.name);
            let url = lib
                .url
                .clone()
                .map(|base| format!("{}/{}", base.trim_end_matches('/'), rel))
                .unwrap_or_default();
            resolved.push(ResolvedLib {
                name: lib.name.clone(),
                rel_path: rel,
                url,
                sha1: String::new(),
                size: 0,
                is_native: false,
                extract_exclude: vec![],
            });
        }
    }

    // Deduplicate classpath entries by groupId:artifactId, keeping highest version.
    dedup_classpath(resolved, libraries_root)
}

/// Pairs of `[preferred, shadowed]` Maven artifact prefixes that publish the
/// *same* Java module name. Keeping both on the module path makes the Forge
/// boot layer die with "reads more than one module named ...".
const SHADOWED_ARTIFACTS: [[&str; 2]; 2] = [
    // both provide module cpw.mods.securejarhandler
    [
        "net.minecraftforge:securemodules:",
        "cpw.mods:securejarhandler:",
    ],
    // Forge 2.x bootstrap replaces the standalone bootstraplauncher
    [
        "net.minecraftforge:bootstrap:",
        "cpw.mods:bootstraplauncher:",
    ],
];

fn dedup_classpath(libs: Vec<ResolvedLib>, _libraries_root: &Path) -> Vec<ResolvedLib> {
    // Partition into natives (no dedup needed) and classpath entries.
    let (natives, cp): (Vec<_>, Vec<_>) = libs.into_iter().partition(|l| l.is_native);

    // Keep the highest version per groupId:artifactId[:classifier] while
    // preserving the original order: classpath order is significant for
    // Minecraft (Forge's patched client must win over vanilla classes).
    let mut order: Vec<String> = Vec::new();
    let mut best: std::collections::HashMap<String, ResolvedLib> = std::collections::HashMap::new();
    for lib in cp {
        let key = coord_key(&lib.name);
        let ver = coord_version(&lib.name).to_owned();
        match best.get(&key) {
            Some(existing) => {
                if version_gt(&ver, coord_version(&existing.name)) {
                    best.insert(key, lib);
                }
            }
            None => {
                order.push(key.clone());
                best.insert(key, lib);
            }
        }
    }

    let mut result: Vec<ResolvedLib> = order
        .iter()
        .filter_map(|key| best.remove(key))
        .collect();

    // Two different coordinates may still resolve to the exact same jar file
    // (loader profiles repeat Mojang entries with their own Maven names).
    // A repeated -cp entry makes libraries like oshi log
    // "Configuration conflict: there is more than one oshi.properties file".
    let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    result.retain(|l| seen_paths.insert(l.rel_path.to_ascii_lowercase()));

    // Drop artifacts whose Java module is already provided by another jar.
    for [preferred, shadowed] in SHADOWED_ARTIFACTS {
        if result.iter().any(|l| l.name.starts_with(preferred)) {
            result.retain(|l| !l.name.starts_with(shadowed));
        }
    }

    result.extend(natives);
    result
}

/// Returns `true` if the library is a Forge 1.21+ module jar that must go on
/// the module path (`-p`) instead of the classpath (`-cp`).
/// ForgeBootstrap and its dependencies (securemodules, unsafe) are required
/// on the module path so the SecureModuleClassLoader can find them.
/// Maven group:artifact prefixes that must be visible on the Java module
/// path. securejarhandler declares a dependency on org.objectweb.asm.tree,
/// so the whole ASM family has to be there too or the boot layer fails with
/// FindException.
const FORGE_MODULE_PREFIXES: &[&str] = &[
    "net.minecraftforge:bootstrap:",
    "net.minecraftforge:bootstrap-api:",
    "net.minecraftforge:securemodules:",
    "net.minecraftforge:unsafe:",
    "net.minecraftforge:JarJarFileSystems:",
    "net.minecraftforge:JarJarSelector:",
    "net.minecraftforge:JarJarMetadata:",
    "cpw.mods:securejarhandler:",
    "cpw.mods:bootstraplauncher:",
    "org.ow2.asm:asm:",
    "org.ow2.asm:asm-commons:",
    "org.ow2.asm:asm-tree:",
    "org.ow2.asm:asm-util:",
    "org.ow2.asm:asm-analysis:",
];

pub fn is_forge_module(lib: &ResolvedLib) -> bool {
    FORGE_MODULE_PREFIXES
        .iter()
        .any(|p| lib.name.starts_with(p))
}

/// Builds the module path string from Forge bootstrap module jars.
/// These go on `-p` (module path) instead of `-cp`.
pub fn build_modulepath(
    libs: &[ResolvedLib],
    libraries_root: &Path,
) -> String {
    // A module may only appear once on -p, otherwise the JVM refuses to
    // build the boot layer.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let parts: Vec<String> = libs
        .iter()
        .filter(|l| !l.is_native && is_forge_module(l))
        .filter(|l| seen.insert(coord_key(&l.name)))
        .map(|l| {
            libraries_root
                .join(&l.rel_path)
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    parts.join(";")
}

/// Builds the classpath string from the resolved non-native libraries plus
/// the client jar. Module jars stay on the classpath too, exactly like the
/// official launcher does, so Fabric and older Forge keep working.
/// Uses `;` as separator (Windows).
pub fn build_classpath(
    libs: &[ResolvedLib],
    libraries_root: &Path,
    client_jar: &Path,
) -> String {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut parts: Vec<String> = Vec::new();
    for lib in libs.iter().filter(|l| !l.is_native) {
        let path = libraries_root
            .join(&lib.rel_path)
            .to_string_lossy()
            .into_owned();
        if seen.insert(path.to_ascii_lowercase()) {
            parts.push(path);
        }
    }
    let client = client_jar.to_string_lossy().into_owned();
    if seen.insert(client.to_ascii_lowercase()) {
        parts.push(client);
    }
    parts.join(";")
}

/// Same as [`maven_path`], but forces the given classifier (used by the legacy
/// `natives.windows` format, where the classifier lives outside the name).
fn maven_path_with_classifier(name: &str, classifier: &str) -> String {
    let Some(coord) = parse_coord(name) else {
        return format!("{}-{classifier}.jar", name.replace([':', '@'], "/"));
    };
    let group = coord.group.replace('.', "/");
    let artifact = coord.artifact;
    let version = coord.version;
    let ext = coord.extension;
    format!("{group}/{artifact}/{version}/{artifact}-{version}-{classifier}.{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::{Rule, RuleOs};

    fn rule(action: &str, os_name: Option<&str>) -> Rule {
        Rule {
            action: action.to_owned(),
            os: os_name.map(|n| RuleOs {
                name: Some(n.to_owned()),
                version: None,
                arch: None,
            }),
            features: None,
        }
    }

    fn lib(name: &str) -> ResolvedLib {
        ResolvedLib {
            name: name.to_owned(),
            rel_path: maven_path(name),
            url: String::new(),
            sha1: String::new(),
            size: 0,
            is_native: false,
            extract_exclude: vec![],
        }
    }

    #[test]
    fn empty_rules_allow() {
        assert!(evaluate_rules(&[]));
    }

    #[test]
    fn allow_all_then_disallow_windows() {
        let rules = vec![rule("allow", None), rule("disallow", Some("windows"))];
        // On Windows this should be false; on other OS true.
        // We just verify the logic runs without panic.
        let _ = evaluate_rules(&rules);
    }

    #[test]
    fn allow_windows_only() {
        let rules = vec![rule("allow", Some("windows"))];
        // On Windows → true; on linux/mac → false.
        let result = evaluate_rules(&rules);
        if cfg!(target_os = "windows") {
            assert!(result);
        } else {
            assert!(!result);
        }
    }

    #[test]
    fn maven_path_correct() {
        assert_eq!(
            maven_path("com.google.guava:guava:21.0"),
            "com/google/guava/guava/21.0/guava-21.0.jar"
        );
        assert_eq!(
            maven_path("net.minecraft:launchwrapper:1.12"),
            "net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar"
        );
    }

    #[test]
    fn maven_path_handles_classifier_and_extension() {
        // Forge install profile coordinate that used to produce an illegal
        // Windows folder name (`1.21:mappings@tsrg`).
        assert_eq!(
            maven_path("net.minecraft:client:1.21:mappings@tsrg"),
            "net/minecraft/client/1.21/client-1.21-mappings.tsrg"
        );
        assert_eq!(
            maven_path("net.minecraft:client:1.21:srg"),
            "net/minecraft/client/1.21/client-1.21-srg.jar"
        );
        assert_eq!(
            maven_path("de.oceanlabs.mcp:mcp_config:1.21@zip"),
            "de/oceanlabs/mcp/mcp_config/1.21/mcp_config-1.21.zip"
        );
    }

    #[test]
    fn maven_paths_never_contain_colons() {
        for name in [
            "net.minecraft:client:1.21:mappings@tsrg",
            "net.minecraftforge:forge:1.21-51.0.33:client",
            "de.oceanlabs.mcp:mcp_config:1.21@zip",
        ] {
            assert!(!maven_path(name).contains(':'), "{name}");
            assert!(!maven_path(name).contains('@'), "{name}");
        }
    }

    #[test]
    fn shadowed_module_artifact_is_dropped() {
        let libs = vec![
            lib("net.minecraftforge:securemodules:2.2.0"),
            lib("cpw.mods:securejarhandler:2.1.10"),
            lib("org.ow2.asm:asm-tree:9.7"),
        ];
        let out = dedup_classpath(libs, Path::new("."));
        assert!(out.iter().all(|l| !l.name.starts_with("cpw.mods:securejarhandler:")));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn classifier_variants_survive_dedup() {
        // The patched client jar and the universal jar share
        // group:artifact:version and differ only by classifier. Dropping
        // either one breaks the Forge boot ("Could not find
        // net/minecraft/client/Minecraft.class").
        let libs = vec![
            lib("net.minecraftforge:forge:1.21-51.0.33:universal"),
            lib("net.minecraftforge:forge:1.21-51.0.33:client"),
            lib("net.minecraft:client:1.21-20240613.145743:extra"),
            lib("net.minecraft:client:1.21-20240613.145743:srg"),
        ];
        let out = dedup_classpath(libs, Path::new("."));
        assert_eq!(out.len(), 4);
        assert!(out.iter().any(|l| l.name.ends_with(":client")));
    }

    #[test]
    fn same_artifact_without_classifier_is_deduplicated() {
        let libs = vec![
            lib("org.ow2.asm:asm:9.5"),
            lib("org.ow2.asm:asm:9.7"),
        ];
        let out = dedup_classpath(libs, Path::new("."));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "org.ow2.asm:asm:9.7");
    }

    #[test]
    fn classpath_has_no_duplicate_files() {
        // Same jar reached through two different coordinates.
        let mut a = lib("com.github.oshi:oshi-core:6.4.10");
        let mut b = lib("com.github.oshi:oshi-core-shaded:6.4.10");
        a.rel_path = "com/github/oshi/oshi-core/6.4.10/oshi-core-6.4.10.jar".to_owned();
        b.rel_path = a.rel_path.clone();
        let cp = build_classpath(&[a, b], Path::new("L"), Path::new("client.jar"));
        assert_eq!(cp.split(';').count(), 2);
    }

    #[test]
    fn modulepath_has_no_duplicate_modules() {
        let libs = vec![
            lib("net.minecraftforge:securemodules:2.2.0"),
            lib("net.minecraftforge:securemodules:2.2.0"),
            lib("org.ow2.asm:asm-tree:9.7"),
        ];
        let mp = build_modulepath(&libs, Path::new("L"));
        assert_eq!(mp.split(';').count(), 2);
    }

    #[test]
    fn coord_key_and_version_ignore_extension() {
        assert_eq!(
            coord_key("net.minecraft:client:1.21:mappings@tsrg"),
            "net.minecraft:client:mappings"
        );
        assert_eq!(coord_version("net.minecraft:client:1.21:mappings@tsrg"), "1.21");
        assert_eq!(coord_key("com.google.guava:guava:21.0"), "com.google.guava:guava");
        assert_eq!(coord_version("com.google.guava:guava:21.0"), "21.0");
    }
}
