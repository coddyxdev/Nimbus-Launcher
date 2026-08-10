import {
	isPermissionGranted,
	requestPermission,
	sendNotification,
} from "@tauri-apps/plugin-notification"

/** Cached so a denied permission is not re-requested on every install. */
let granted: boolean | null = null

async function ensurePermission(): Promise<boolean> {
	if (granted !== null) return granted
	granted = await isPermissionGranted()
	if (!granted) {
		granted = (await requestPermission()) === "granted"
	}
	return granted
}

/**
 * Shows a Windows notification, but only when the launcher is in the
 * background: if the user is already looking at the window, the in-app toast
 * has said everything and a second popup is just noise.
 *
 * Notifications are a nicety, so every failure here is swallowed.
 */
export async function notifyInBackground(title: string, body: string) {
	if (document.visibilityState === "visible" && document.hasFocus()) return
	try {
		if (!(await ensurePermission())) return
		sendNotification({ title, body })
	} catch {
		/* no notification is better than a broken install flow */
	}
}
