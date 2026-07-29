import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

export type UpdateInfo = {
  version: string;
  notes: string | null;
};

/** Outcome of an update check. Failures are reported, never silently dropped. */
export type UpdateCheck =
  | { status: "available"; info: UpdateInfo }
  | { status: "current" }
  | { status: "unconfigured" }
  | { status: "failed"; message: string };

/** Mirrors the event shape passed to `Update.downloadAndInstall`. */
type DownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

/** The `Update` handle from the last successful `checkForUpdate` call. */
let pending: Update | null = null;

/**
 * A shipped build whose `tauri.conf.json` still carries the template
 * placeholders can never verify a signature, so every check fails with an
 * opaque error. Detecting it explicitly turns "updates silently never arrive"
 * into an actionable message in the developer log.
 */
function looksUnconfigured(message: string): boolean {
  return (
    message.includes("REPLACE_OWNER") ||
    message.includes("REPLACE_REPO") ||
    message.includes("REPLACE_WITH_YOUR_PUBLIC_KEY")
  );
}

/**
 * Checks GitHub Releases (see `tauri.conf.json` → `plugins.updater.endpoints`)
 * for a newer signed build.
 *
 * Never throws: it runs on boot and must not block startup. The caller decides
 * what to do with a failure — currently it is written to the developer log
 * instead of being shown to every user.
 */
export async function checkForUpdate(): Promise<UpdateCheck> {
  try {
    const update = await check();
    if (!update?.available) return { status: "current" };
    pending = update;
    return {
      status: "available",
      info: { version: update.version, notes: update.body ?? null },
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return looksUnconfigured(message)
      ? { status: "unconfigured" }
      : { status: "failed", message };
  }
}

/**
 * Downloads and installs the update found by the last `checkForUpdate` call,
 * then restarts the app into the new version. Throws on failure so the
 * caller can show an error toast.
 */
export async function installPendingUpdate(
  onProgress?: (pct: number) => void,
): Promise<void> {
  if (!pending) return;
  let downloaded = 0;
  let total = 0;
  await pending.downloadAndInstall((event: DownloadEvent) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? 0;
    } else if (event.event === "Progress") {
      downloaded += event.data.chunkLength;
      if (total > 0) onProgress?.(Math.round((downloaded / total) * 100));
    }
  });
  await relaunch();
}
