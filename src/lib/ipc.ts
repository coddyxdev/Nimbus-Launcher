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

/** Payload of the `game:output` event. */
export type GameOutput = {
  instanceId: string;
  line: string;
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

/** Result of a shared-cache cleanup pass. */
export type CleanupReport = {
  removedFiles: number;
  freedBytes: number;
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
};

/** Which Java the launcher will actually use for a given major version. */
export type JavaInfo = {
  path: string;
  /** True when the runtime was downloaded and is managed by the launcher. */
  isManaged: boolean;
  /** True when it comes from the explicit setting rather than detection. */
  isOverride: boolean;
};

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
    call<Instance>("import_modpack", { path, instanceName: instanceName ?? null }),
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
  listMods: (instanceId: string) =>
    call<ModInfo[]>("list_mods", { instanceId }),
  addMod: (instanceId: string, sourcePath: string) =>
    call<ModInfo>("add_mod", { instanceId, sourcePath }),
  removeMod: (instanceId: string, fileName: string) =>
    call<void>("remove_mod", { instanceId, fileName }),
  setModEnabled: (instanceId: string, fileName: string, enabled: boolean) =>
    call<ModInfo>("set_mod_enabled", { instanceId, fileName, enabled }),
  modrinthSearch: (instanceId: string, query: string, limit?: number) =>
    call<ModrinthHit[]>("modrinth_search", { instanceId, query, limit: limit ?? null }),
  modrinthVersions: (instanceId: string, projectId: string) =>
    call<ModrinthVersion[]>("modrinth_versions", { instanceId, projectId }),
  modrinthInstall: (instanceId: string, projectId: string, versionId?: string) =>
    call<ModInfo>("modrinth_install", { instanceId, projectId, versionId: versionId ?? null }),
  openGameDir: (instanceId: string) =>
    call<void>("open_game_dir", { instanceId }),
  openModsDir: (instanceId: string) =>
    call<void>("open_mods_dir", { instanceId }),
  openLogsDir: (instanceId: string) =>
    call<void>("open_logs_dir", { instanceId }),
  saveTextFile: (path: string, contents: string) =>
    call<void>("save_text_file", { path, contents }),
  cleanupShared: () => call<CleanupReport>("cleanup_shared"),
  deleteInstance: (instanceId: string) =>
    call<void>("delete_instance", { instanceId }),
  launchInstance: (instanceId: string) =>
    call<LaunchResult>("launch_instance", { instanceId }),
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
  /** Reports the Java binary that would be used for `majorVersion`. */
  resolveJava: (majorVersion: number) =>
    call<JavaInfo>("resolve_java", { majorVersion }),

  // ── Microsoft account ────────────────────────────────────────────────────
  /** Stores the Azure application id. An empty string clears it. */
  setAzureClientId: (clientId: string) =>
    call<Config>("set_azure_client_id", { clientId }),
  /** Starts sign-in; returns the code to show the user. */
  beginMsLogin: () => call<DeviceCode>("begin_ms_login"),
  /** Resolves once the user finishes in the browser. May take minutes. */
  completeMsLogin: () => call<AccountInfo>("complete_ms_login"),
  cancelMsLogin: () => call<void>("cancel_ms_login"),
  getAccount: () => call<AccountInfo | null>("get_account"),
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
