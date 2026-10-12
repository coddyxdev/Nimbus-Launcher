import {
	isPermissionGranted,
	requestPermission,
	sendNotification,
} from "@tauri-apps/plugin-notification"

/**
 * Cached only once permission is actually granted. A denial is not cached: the
 * user can grant it in the OS at any time, and re-probing is cheap compared to
 * silently never notifying again for the rest of the session.
 */
let granted = false

async function ensurePermission(): Promise<boolean> {
	if (granted) return true
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
		// Awaited so a rejection lands in the catch below instead of surfacing
		// as an unhandled promise rejection.
		await sendNotification({ title, body })
	} catch {
		/* no notification is better than a broken install flow */
	}
}
