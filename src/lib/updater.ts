import { updater, type UpdateInfo } from "./updater.svelte"

export type { UpdateInfo }
export type UpdateCheck =
	| { status: "available"; info: UpdateInfo }
	| { status: "current" }
	| { status: "unconfigured" }
	| { status: "failed"; message: string }

export async function checkForUpdate(): Promise<UpdateCheck> {
	await updater.check()
	if (updater.status === "available" && updater.version) {
		return { status: "available", info: { version: updater.version, notes: updater.notes } }
	}
	if (updater.status === "unconfigured") return { status: "unconfigured" }
	if (updater.status === "failed") return { status: "failed", message: updater.error ?? "" }
	return { status: "current" }
}

export async function installPendingUpdate(): Promise<void> {
	return updater.install()
}
