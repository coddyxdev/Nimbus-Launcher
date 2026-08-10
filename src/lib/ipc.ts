import { invoke } from "@tauri-apps/api/core";

/** Mirrors `NimbusError` on the Rust side. */
export type NimbusError = {
  kind: string;
  message: string;
  /** True when retrying the same action can plausibly succeed (network, 5xx). */
  retriable?: boolean;
};

export type Theme = "dark" | "light" | "system";

export type Config = {
  version: number;
  theme: Theme;
  defaultMemoryMib: number;
  defaultJvmArgs: string[];
  defaultAikarFlags: boolean;
  offlineUsername: string | null;
  azureClientId: string | null;
  onboardingDone: boolean;
  /** Explicit javaw.exe path, or null for auto-detection (config v3+). */
  javaPath: string | null;
  /** Initial game window size; null lets Minecraft decide (config v3+). */
  gameWidth: number | null;
  gameHeight: number | null;
  gameFullscreen: boolean;
  /** Publish "playing <instance>" to Discord (config v3+). */
  discordRpc: boolean;
  /** Custom background file inside the profile, or null (config v4+). */
  backgroundFile: string | null;
  /** "image" or "video" (config v4+). */
  backgroundKind: string | null;
  /** How strongly the background shows through, 1..100 (config v4+). */
  backgroundOpacity: number;
  /** Background blur radius in px, 0..40 (config v4+). */
  backgroundBlur: number;
};

/** The picture or clip currently used as the launcher background. */
export type BackgroundInfo = {
  fileName: string;
  /** Absolute path; render it through convertFileSrc(). */
  path: string;
  /** "image" (png/jpg/gif/webp) or "video" (mp4/webm). */
  kind: string;
  sizeBytes: number;
};

/** What the user must enter to finish Microsoft sign-in. */
export type DeviceCode = {
  userCode: string;
  verificationUri: string;
  deviceCode: string;
  expiresIn: number;
  interval: number;
};

/** A signed-in Microsoft account. Tokens never reach the frontend. */
export type AccountInfo = {
  uuid: string;
  name: string;
  expiresAt: number;
};

/** An importable Prism Launcher / MultiMC instance found on disk. */
export type PrismCandidate = {
  path: string;
  name: string;
  minecraftVersion: string;
  loader: string | null;
  loaderVersion: string | null;
  modsCount: number;
  sizeBytes: number;
  playedSecs: number;
};

export type Bootstrap = {
  config: Config;
  launcherVersion: string;
  dataDir: string;
  authUnavailable: boolean;
};

/** Per-instance launch overrides. `null` means "use the global default". */
export type InstanceSettings = {
  memoryMib?: number | null;
  jvmArgs?: string[] | null;
  aikarFlags?: boolean | null;
};

/** Which Modrinth project/version an instance's modpack was installed from. */
export type ModpackSource = {
  projectId: string;
  versionId: string;
};

/** Mirrors `instance::Instance` on the Rust side. */
export type Instance = {
  id: string;
  name: string;
  versionId: string;
  loader: string | null;
  loaderVersion: string | null;
  minecraftVersion: string | null;
  createdAt: number;
  lastPlayed: number | null;
  /** `false` while an install is unfinished. Missing on pre-1.2 instances. */
  installed?: boolean | null;
  settings?: InstanceSettings | null;
  /** Accumulated play time in seconds across all sessions. */
  totalPlaytimeSecs?: number | null;
  /** Pinned to the top of the sidebar list. */
  favorite?: boolean | null;
  /** Set when this instance's modpack was installed from Modrinth; enables update checks. */
  modpackSource?: ModpackSource | null;
};

/** Legacy instances have no flag and count as installed. */
export function isInstalled(instance: Instance): boolean {
  return instance.installed !== false;
}

/** One entry of the Mojang version manifest. */
export type VersionSummary = {
  id: string;
  type: "release" | "snapshot" | "old_beta" | "old_alpha" | string;
  releaseTime: string;
};

/** Payload of the `install:progress` event. */
export type InstallProgress = {
  stage: string;
  file: string;
  done: number;
  total: number;
  bytesDone: number;
  bytesTotal: number;
};

/**
 * Payload of the `game:output` event. Carries a small batch of lines instead
 * of one per event: a modded Forge instance can print thousands of lines a
 * second at startup, and emitting (plus flushing two log files for) each one
 * individually floods the IPC bridge and stalls the WebView.
 */
export type GameOutput = {
  instanceId: string;
  lines: string[];
  stream: "out" | "err";
};

/**
 * Payload of the `game:exit` event.
 *
 * Emitted exactly once per launch, by the process watcher task.
 * `killedByUser` is true when the exit is the result of "Stop" in the UI, so
 * the frontend must not report it as a crash.
 */
export type GameExit = {
  instanceId: string;
  code: number;
  killedByUser: boolean;
  /** Wall-clock seconds the process was alive for this launch. */
  playedSeconds: number;
};

export type LaunchResult = {
  pid: number;
};

/** A mod loader version returned from the backend. */
export type LoaderVersionInfo = {
  version: string;
  stable: boolean;
};

/** Canonical mod loader name. */
export type ModLoader = "fabric" | "quilt" | "forge" | "neoforge";

/** Information about a single .jar mod file in an instance. */
export type ModInfo = {
  /** Always the enabled name, even when the file on disk ends with .disabled. */
  fileName: string;
  sizeBytes: number;
  lastModified: number;
  enabled: boolean;
};

/** A crash report file in the instance's crash-reports directory. */
export type CrashReportInfo = {
  fileName: string;
  sizeBytes: number;
  lastModified: number;
};

/** One diagnosis produced by the crash analyzer for a single matched pattern. */
export type CrashFinding = {
  title: string;
  detail: string;
  suggestion: string;
};

/** Result of running the heuristic crash analyzer over a crash report. */
export type CrashAnalysis = {
  findings: CrashFinding[];
  /** Mod names the analyzer could pull out of the report text, if any. */
  suspectedMods: string[];
};

/** Result of a shared-cache cleanup pass. */
export type CleanupReport = {
  removedFiles: number;
  freedBytes: number;
};

/** Whether a newer Modrinth version exists for an instance's modpack. */
export type ModpackUpdateInfo = {
  hasUpdate: boolean;
  currentVersionId: string;
  latestVersionId: string;
  latestVersionName: string;
};

/** Partial config update, sent to `update_config`. */
export type ConfigUpdate = {
  theme?: Theme;
  defaultMemoryMib?: number;
  defaultJvmArgs?: string[];
  defaultAikarFlags?: boolean;
  offlineUsername?: string;
  /** Empty string clears the override and restores auto-detection. */
  javaPath?: string;
  /** Zero clears the override. */
  gameWidth?: number;
  gameHeight?: number;
  gameFullscreen?: boolean;
  discordRpc?: boolean;
  /** Background strength in percent, 1..100. */
  backgroundOpacity?: number;
  /** Background blur radius in px, 0..40. */
  backgroundBlur?: number;
};

/** Which Java the launcher will actually use for a given major version. */
export type JavaInfo = {
  path: string;
  /** True when the runtime was downloaded and is managed by the launcher. */
  isManaged: boolean;
  /** True when it comes from the explicit setting rather than detection. */
  isOverride: boolean;
};

/** Sort order for Modrinth catalogue search, mirroring the site's own sort dropdown. */
export type ModrinthSort = "relevance" | "downloads" | "follows" | "newest" | "updated";

/**
 * Modrinth payloads keep the upstream snake_case field names, because they are
 * passed through from the API response verbatim.
 */
export type ModrinthHit = {
  project_id: string;
  slug: string;
  title: string;
  description: string;
  downloads: number;
  icon_url: string | null;
  author: string | null;
  client_side: string | null;
};

/** One screenshot from a project's gallery. */
export type ModrinthGalleryItem = {
  url: string;
  featured: boolean;
  title: string | null;
  description: string | null;
};

/** Full project page, as shown in the in-app mod details view. */
export type ModrinthProject = {
  id: string;
  slug: string;
  title: string;
  description: string;
  /** Long description in Markdown, exactly as authored on Modrinth. */
  body: string;
  project_type: string;
  categories: string[];
  downloads: number;
  followers: number;
  icon_url: string | null;
  issues_url: string | null;
  source_url: string | null;
  wiki_url: string | null;
  discord_url: string | null;
  client_side: string | null;
  server_side: string | null;
  game_versions: string[];
  loaders: string[];
  published: string | null;
  updated: string | null;
  license: { id: string | null; name: string | null; url: string | null } | null;
  gallery: ModrinthGalleryItem[];
};

export type ModrinthFile = {
  url: string;
  filename: string;
  size: number;
  primary: boolean;
  hashes: { sha1: string | null };
};

export type ModrinthVersion = {
  id: string;
  name: string;
  version_number: string;
  version_type: string;
  game_versions: string[];
  loaders: string[];
  date_published: string | null;
  files: ModrinthFile[];
};

/** One post of the launcher news feed. Mirrors `NewsItem` in Rust. */
export type NewsItem = {
  id: string;
  /** ISO date, exactly as written in news.json. */
  date: string;
  titleRu: string;
  titleEn: string;
  bodyRu: string;
  bodyEn: string;
  /** Optional "read more" target, opened in the system browser. */
  link: string | null;
};

function toNimbusError(err: unknown): NimbusError {
  if (
    typeof err === "object" &&
    err !== null &&
    "kind" in err &&
    "message" in err
  ) {
    return err as NimbusError;
  }
  return { kind: "unknown", message: String(err) };
}

/** True when the user pressed "Cancel" during an install. */
export function isCancelled(err: unknown): boolean {
  return toNimbusError(err).kind === "cancelled";
}

async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    throw toNimbusError(err);
  }
}

export const ipc = {
  bootstrap: () => call<Bootstrap>("bootstrap"),
  setTheme: (theme: Theme) => call<Config>("set_theme", { theme }),
  setOfflineUsername: (username: string) =>
    call<Config>("set_offline_username", { username }),
  completeOnboarding: () => call<Config>("complete_onboarding"),
  updateConfig: (update: ConfigUpdate) =>
    call<Config>("update_config", { update }),
  /** The launcher background in use, or null when none is set. */
  getBackground: () => call<BackgroundInfo | null>("get_background"),
  /** Copies a picture or clip into the profile and makes it the background. */
  setBackground: (sourcePath: string) =>
    call<BackgroundInfo>("set_background", { sourcePath }),
  /** Removes the background and deletes the copy inside the profile. */
  clearBackground: () => call<void>("clear_background"),
  getGameLog: (instanceId: string) =>
    call<string[]>("get_game_log", { instanceId }),
  gameLogPath: (instanceId: string) =>
    call<string>("game_log_path", { instanceId }),
  listInstances: () => call<Instance[]>("list_instances"),
  listVersions: (includeSnapshots: boolean) =>
    call<VersionSummary[]>("list_versions", { includeSnapshots }),
  listLoaderVersions: (loader: ModLoader, mcVersion: string) =>
    call<LoaderVersionInfo[]>("list_loader_versions", { loader, mcVersion }),
  installVersion: (versionId: string, instanceName: string, loader?: ModLoader, loaderVersion?: string) =>
    call<Instance>("install_version", { versionId, instanceName, loader: loader ?? null, loaderVersion: loaderVersion ?? null }),
  installLoader: (instanceId: string, loader: ModLoader, loaderVersion: string) =>
    call<Instance>("install_loader", { instanceId, loader, loaderVersion }),
  /** Imports a .mrpack (Modrinth) modpack: installs the base game then layers mods/configs/overrides on top. */
  importModpack: (path: string, instanceName?: string) =>
    call<Instance>("import_modpack", {
      path,
      instanceName: instanceName ?? null,
      projectId: null,
      versionId: null,
    }),
  /** Exports an instance's game files + metadata to a portable .zip backup file. */
  exportInstance: (instanceId: string, destPath: string) =>
    call<void>("export_instance", { instanceId, destPath }),
  /** Imports a .zip backup previously created by exportInstance as a new instance. */
  importInstance: (path: string, instanceName?: string) =>
    call<Instance>("import_instance", { path, instanceName: instanceName ?? null }),
  cancelInstall: () => call<void>("cancel_install"),
  verifyInstance: (instanceId: string) =>
    call<number>("verify_instance", { instanceId }),
  instanceSize: (instanceId: string) =>
    call<number>("instance_size", { instanceId }),
  setInstanceSettings: (instanceId: string, settings: InstanceSettings | null) =>
    call<Instance>("set_instance_settings", { instanceId, settings }),
  setInstanceFavorite: (instanceId: string, favorite: boolean) =>
    call<Instance>("set_instance_favorite", { instanceId, favorite }),
  listMods: (instanceId: string) =>
    call<ModInfo[]>("list_mods", { instanceId }),
  addMod: (instanceId: string, sourcePath: string) =>
    call<ModInfo>("add_mod", { instanceId, sourcePath }),
  removeMod: (instanceId: string, fileName: string) =>
    call<void>("remove_mod", { instanceId, fileName }),
  setModEnabled: (instanceId: string, fileName: string, enabled: boolean) =>
    call<ModInfo>("set_mod_enabled", { instanceId, fileName, enabled }),
  /** Full project page (long description, gallery, links) for the details view. */
  modrinthProject: (projectId: string) =>
    call<ModrinthProject>("modrinth_project", { projectId }),
  /** Every published version of a project, without instance filtering. */
  modrinthProjectVersions: (projectId: string) =>
    call<ModrinthVersion[]>("modrinth_project_versions", { projectId }),
  modrinthSearch: (instanceId: string, query: string, limit?: number, sort?: ModrinthSort) =>
    call<ModrinthHit[]>("modrinth_search", {
      instanceId,
      query,
      limit: limit ?? null,
      sort: sort ?? null,
    }),
  modrinthVersions: (instanceId: string, projectId: string) =>
    call<ModrinthVersion[]>("modrinth_versions", { instanceId, projectId }),
  modrinthInstall: (instanceId: string, projectId: string, versionId?: string) =>
    call<ModInfo>("modrinth_install", { instanceId, projectId, versionId: versionId ?? null }),
  /** Hashes installed jars and reports which ones have a newer version. */
  checkModUpdates: (instanceId: string) =>
    call<ModUpdate[]>("check_mod_updates", { instanceId }),
  /** Replaces one jar with a newer version, keeping its enabled/disabled state. */
  applyModUpdate: (instanceId: string, fileName: string, versionId: string) =>
    call<string>("apply_mod_update", { instanceId, fileName, versionId }),
  /** Updates every mod that has a newer version. */
  applyAllModUpdates: (instanceId: string) =>
    call<InstallWithDepsReport>("apply_all_mod_updates", { instanceId }),
  /** What a mod version needs, and what is already installed. */
  modDependencies: (instanceId: string, projectId: string, versionId?: string) =>
    call<ModDependency[]>("mod_dependencies", {
      instanceId,
      projectId,
      versionId: versionId ?? null,
    }),
  /** Installs a mod together with its missing dependencies. */
  installModWithDeps: (
    instanceId: string,
    projectId: string,
    versionId?: string,
    includeOptional?: boolean,
  ) =>
    call<InstallWithDepsReport>("install_mod_with_deps", {
      instanceId,
      projectId,
      versionId: versionId ?? null,
      includeOptional: includeOptional ?? false,
    }),
  /** Exports an instance as a shareable .mrpack. */
  exportMrpack: (instanceId: string, destPath: string, versionName?: string) =>
    call<ExportReport>("export_mrpack", {
      instanceId,
      destPath,
      versionName: versionName ?? null,
    }),
  /** Screenshots of an instance, newest first. Paths feed convertFileSrc. */
  listScreenshots: (instanceId: string) =>
    call<Screenshot[]>("list_screenshots", { instanceId }),
  deleteScreenshot: (instanceId: string, fileName: string) =>
    call<void>("delete_screenshot", { instanceId, fileName }),
  /** Copies a screenshot to a user-picked path — the "share" action. */
  copyScreenshot: (instanceId: string, fileName: string, destPath: string) =>
    call<string>("copy_screenshot", { instanceId, fileName, destPath }),
  /** Snapshots the mods folder so a bad update can be undone. */
  createRestorePoint: (instanceId: string, label: string) =>
    call<RestorePoint>("create_restore_point", { instanceId, label }),
  listRestorePoints: (instanceId: string) =>
    call<RestorePoint[]>("list_restore_points", { instanceId }),
  /** Rolls the mods folder back; snapshots the current state first. */
  applyRestorePoint: (instanceId: string, pointId: string) =>
    call<RestorePoint>("apply_restore_point", { instanceId, pointId }),
  deleteRestorePoint: (instanceId: string, pointId: string) =>
    call<void>("delete_restore_point", { instanceId, pointId }),
  /** Searches Modrinth modpacks (as opposed to mods), for the "install from Modrinth" flow. */
  modrinthSearchModpacks: (
    query: string,
    loader?: ModLoader,
    mcVersion?: string,
    sort?: ModrinthSort,
  ) =>
    call<ModrinthHit[]>("modrinth_search_modpacks", {
      query,
      loader: loader ?? null,
      mcVersion: mcVersion ?? null,
      sort: sort ?? null,
    }),
  /** Downloads the newest compatible version of a Modrinth modpack and installs it as a new instance. */
  installModpackFromModrinth: (projectId: string, instanceName?: string) =>
    call<Instance>("install_modpack_from_modrinth", {
      projectId,
      instanceName: instanceName ?? null,
    }),
  /** Checks whether a newer version exists for an instance installed from Modrinth. Errors if it wasn't. */
  checkModpackUpdate: (instanceId: string) =>
    call<ModpackUpdateInfo>("check_modpack_update", { instanceId }),
  /** Downloads and applies the newest Modrinth version over an existing instance. */
  updateModpack: (instanceId: string) =>
    call<Instance>("update_modpack", { instanceId }),
  openGameDir: (instanceId: string) =>
    call<void>("open_game_dir", { instanceId }),
  openModsDir: (instanceId: string) =>
    call<void>("open_mods_dir", { instanceId }),
  openLogsDir: (instanceId: string) =>
    call<void>("open_logs_dir", { instanceId }),
  /** Opens the launcher's own log folder, where launcher.log lives. */
  openLauncherLogsDir: () => call<void>("open_launcher_logs_dir"),
  /** Opens an external link in the system browser. */
  openUrl: (url: string) => call<void>("open_url", { url }),
  /** Launcher news. Fetched in Rust: the webview CSP forbids the request. */
  fetchNews: () => call<NewsItem[]>("fetch_news"),
  saveTextFile: (path: string, contents: string) =>
    call<void>("save_text_file", { path, contents }),
  cleanupShared: () => call<CleanupReport>("cleanup_shared"),
  deleteInstance: (instanceId: string) =>
    call<void>("delete_instance", { instanceId }),
  /** `server` joins that address on start; omit it for the main menu. */
  launchInstance: (instanceId: string, server?: string | null) =>
    call<LaunchResult>("launch_instance", { instanceId, server: server ?? null }),
  /** Disk usage of every instance plus the shared cache. */
  storageUsage: () => call<StorageUsage>("storage_usage"),
  /** Known-bad mod pairs found in the instance's mods folder. */
  checkModConflicts: (instanceId: string) =>
    call<CrashFinding[]>("check_mod_conflicts", { instanceId }),
  /** Suggests a heap size from the machine's RAM and the build's mod count. */
  recommendMemory: (instanceId: string) =>
    call<MemoryAdvice>("recommend_memory", { instanceId }),
  /** Multiplayer list from the instance's own servers.dat. */
  listServers: (instanceId: string) =>
    call<ServerEntry[]>("list_servers", { instanceId }),
  addServer: (instanceId: string, name: string, address: string) =>
    call<ServerEntry[]>("add_server", { instanceId, name, address }),
  removeServer: (instanceId: string, address: string) =>
    call<ServerEntry[]>("remove_server", { instanceId, address }),
  /** Server List Ping. A dead server returns online: false instead of throwing. */
  pingServer: (address: string) => call<ServerStatus>("ping_server", { address }),
  killInstance: (instanceId: string) =>
    call<void>("kill_instance", { instanceId }),
  duplicateInstance: (instanceId: string, newName: string) =>
    call<Instance>("duplicate_instance", { instanceId, newName }),
  renameInstance: (instanceId: string, newName: string) =>
    call<Instance>("rename_instance", { instanceId, newName }),
  openScreenshotsDir: (instanceId: string) =>
    call<void>("open_screenshots_dir", { instanceId }),
  openCrashReportsDir: (instanceId: string) =>
    call<void>("open_crash_reports_dir", { instanceId }),
  listCrashReports: (instanceId: string) =>
    call<CrashReportInfo[]>("list_crash_reports", { instanceId }),
  readCrashReport: (instanceId: string, fileName: string) =>
    call<string>("read_crash_report", { instanceId, fileName }),
  /** Reads a crash report and runs the heuristic analyzer over it in one call. */
  analyzeCrashReport: (instanceId: string, fileName: string) =>
    call<CrashAnalysis>("analyze_crash_report", { instanceId, fileName }),
  /** Reports the Java binary that would be used for `majorVersion`. */
  resolveJava: (majorVersion: number) =>
    call<JavaInfo>("resolve_java", { majorVersion }),

  // ── Microsoft account ────────────────────────────────────────────────────
  /** Stores the Azure application id. An empty string clears it. */
  setAzureClientId: (clientId: string) =>
    call<Config>("set_azure_client_id", { clientId }),
  /** Starts sign-in; returns the code to show the user. */
  beginMsLogin: () => call<DeviceCode>("begin_ms_login"),
  /** Resolves once the user finishes in the browser. May take minutes. Adds the account alongside any already signed in. */
  completeMsLogin: () => call<AccountInfo>("complete_ms_login"),
  cancelMsLogin: () => call<void>("cancel_ms_login"),
  /** Opens the Microsoft page for the sign-in in progress. */
  openLoginPage: () => call<void>("open_login_page"),
  /** The currently active account, or null in offline mode. */
  getAccount: () => call<AccountInfo | null>("get_account"),
  /** Every signed-in account, active one first. */
  listAccounts: () => call<AccountInfo[]>("list_accounts"),
  /** Makes an already signed-in account active. No network calls needed. */
  switchAccount: (uuid: string) => call<AccountInfo>("switch_account", { uuid }),
  /** Removes one signed-in account; another remaining one becomes active automatically. */
  removeAccount: (uuid: string) =>
    call<AccountInfo | null>("remove_account", { uuid }),
  /** Signs out completely: removes every signed-in account. */
  signOut: () => call<void>("sign_out"),

  // ── Prism / MultiMC import ───────────────────────────────────────────────
  scanPrismInstances: (root: string) =>
    call<PrismCandidate[]>("scan_prism_instances", { root }),
  importPrismInstance: (path: string, instanceName?: string) =>
    call<Instance>("import_prism_instance", {
      path,
      instanceName: instanceName ?? null,
    }),
};

/** An available update for one installed mod. Mirrors `ModUpdate` in Rust. */
export type ModUpdate = {
  /** File on disk, including the .disabled suffix when the mod is off. */
  fileName: string;
  enabled: boolean;
  projectId: string;
  title: string;
  iconUrl: string | null;
  currentVersion: string;
  latestVersion: string;
  latestVersionId: string;
  latestFileName: string;
};

/** One dependency of a mod version, annotated with its install state. */
export type ModDependency = {
  projectId: string;
  title: string;
  iconUrl: string | null;
  /** "required" | "optional" | "incompatible" | "embedded". */
  dependencyType: string;
  installed: boolean;
  versionId: string | null;
};

/** Result of a batch install/update: file names installed, projects skipped. */
export type InstallWithDepsReport = {
  installed: string[];
  skipped: string[];
};

/** A snapshot of an instance's mods folder, taken before a risky change. */
export type RestorePoint = {
  id: string;
  label: string;
  /** Unix seconds. */
  createdAt: number;
  modCount: number;
  sizeBytes: number;
  /** Taken automatically before an update or a rollback. */
  automatic: boolean;
};

/** Disk usage of one instance. */
export type StorageEntry = {
  id: string
  name: string
  bytes: number
}

/** Disk usage of everything the launcher owns. */
export type StorageUsage = {
  instances: StorageEntry[]
  instancesBytes: number
  sharedBytes: number
  totalBytes: number
}

/** Heap advice for one build. */
export type MemoryAdvice = {
  systemMib: number
  availableMib: number
  modCount: number
  recommendedMib: number
  currentMib: number
}

/** One row of the vanilla multiplayer list. */
export type ServerEntry = {
  name: string
  ip: string
  icon?: string | null
  acceptTextures?: number | null
}

/** Live status of a server, as answered by Server List Ping. */
export type ServerStatus = {
  online: boolean
  players: number
  maxPlayers: number
  version: string
  motd: string
  latencyMs: number
  favicon?: string | null
  error?: string | null
}

/** One screenshot from an instance's screenshots folder. */
export type Screenshot = {
  fileName: string;
  /** Absolute path; render it through convertFileSrc(). */
  path: string;
  sizeBytes: number;
  /** Unix seconds. */
  modified: number;
};

/** Outcome of exporting an instance as a .mrpack. */
export type ExportReport = {
  path: string;
  /** Mods resolved to a Modrinth download link. */
  linkedMods: number;
  /** Mods bundled into overrides/ because Modrinth did not know them. */
  bundledMods: number;
  sizeBytes: number;
};
