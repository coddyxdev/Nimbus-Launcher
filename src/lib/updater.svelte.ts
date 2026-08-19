import { relaunch } from "@tauri-apps/plugin-process"
import { check, type Update } from "@tauri-apps/plugin-updater"
import { toasts } from "./toast.svelte"
import { t, tf } from "./i18n.svelte"

export type UpdateInfo = {
	version: string
	notes: string | null
}

export type UpdateCheckStatus = "idle" | "checking" | "available" | "current" | "unconfigured" | "failed"

type DownloadEvent =
	| { event: "Started"; data: { contentLength?: number } }
	| { event: "Progress"; data: { chunkLength: number } }
	| { event: "Finished" }

const AUTO_CHECK_KEY = "nimbus.updater.autoCheck"

class UpdaterStore {
	status = $state<UpdateCheckStatus>("idle")
	checking = $state(false)
	downloading = $state(false)
	progress = $state<number | null>(null)
	version = $state<string | null>(null)
	notes = $state<string | null>(null)
	error = $state<string | null>(null)
	autoCheck = $state(this.readAutoCheck())

	private pending: Update | null = null
	private intervalTimer: ReturnType<typeof setInterval> | null = null
	private onDevLog?: (msg: string) => void

	get available(): boolean {
		return this.status === "available" && this.version !== null
	}

	setDevLogHandler(handler: (msg: string) => void): void {
		this.onDevLog = handler
	}

	private readAutoCheck(): boolean {
		try {
			const val = window.localStorage.getItem(AUTO_CHECK_KEY)
			return val === null ? true : val === "1"
		} catch {
			return true
		}
	}

	setAutoCheck(enabled: boolean): void {
		this.autoCheck = enabled
		try {
			window.localStorage.setItem(AUTO_CHECK_KEY, enabled ? "1" : "0")
		} catch {}
	}

	private looksUnconfigured(message: string): boolean {
		const lower = message.toLowerCase()
		return (
			message.includes("REPLACE_OWNER") ||
			message.includes("REPLACE_REPO") ||
			message.includes("REPLACE_WITH_YOUR_PUBLIC_KEY") ||
			lower.includes("no endpoints") ||
			lower.includes("invalid public key") ||
			lower.includes("pubkey")
		)
	}

	async check(options: { manual?: boolean } = {}): Promise<boolean> {
		if (this.checking || this.downloading) return false
		this.checking = true
		this.error = null
		if (options.manual) {
			this.status = "checking"
		}

		try {
			const update = await check()
			if (!update?.available) {
				this.status = "current"
				this.pending = null
				this.version = null
				this.notes = null
				if (this.onDevLog) this.onDevLog(t("updater: установлена последняя версия"))
				if (options.manual) {
					toasts.success(t("У вас установлена последняя версия"))
				}
				return false
			}

			this.pending = update
			this.version = update.version
			this.notes = update.body ?? null
			this.status = "available"

			const msg = tf("updater: доступна версия {0}", update.version)
			if (this.onDevLog) this.onDevLog(msg)
			if (options.manual) {
				toasts.info(tf("Доступно обновление: {0}", update.version))
			}
			return true
		} catch (err) {
			const message = err instanceof Error ? err.message : String(err)
			if (this.looksUnconfigured(message)) {
				this.status = "unconfigured"
				if (this.onDevLog) {
					this.onDevLog(t("updater: не настроен — задайте endpoints и pubkey в tauri.conf.json"))
				}
			} else {
				this.status = "failed"
				this.error = message
				if (this.onDevLog) {
					this.onDevLog(tf("updater: проверка не удалась — {0}", message))
				}
				if (options.manual) {
					toasts.error(tf("Не удалось проверить обновления: {0}", message))
				}
			}
			return false
		} finally {
			this.checking = false
		}
	}

	async install(): Promise<void> {
		if (!this.pending || this.downloading) return
		this.downloading = true
		this.progress = 0
		this.error = null

		try {
			let downloaded = 0
			let total = 0

			await this.pending.downloadAndInstall((event: DownloadEvent) => {
				if (event.event === "Started") {
					total = event.data.contentLength ?? 0
				} else if (event.event === "Progress") {
					downloaded += event.data.chunkLength
					if (total > 0) {
						this.progress = Math.round((downloaded / total) * 100)
					}
				} else if (event.event === "Finished") {
					this.progress = 100
				}
			})

			await relaunch()
		} catch (err) {
			this.downloading = false
			this.progress = null
			const msg = err instanceof Error ? err.message : String(err)
			this.error = msg
			toasts.error(tf("Не удалось установить обновление: {0}", msg))
			throw err
		}
	}

	startPeriodicCheck(intervalMs = 30 * 60 * 1000): () => void {
		if (this.autoCheck) {
			void this.check()
		}

		if (this.intervalTimer) clearInterval(this.intervalTimer)
		this.intervalTimer = setInterval(() => {
			if (this.autoCheck && !this.downloading) {
				void this.check()
			}
		}, intervalMs)

		const onFocus = () => {
			if (this.autoCheck && !this.downloading && this.status !== "available") {
				void this.check()
			}
		}
		window.addEventListener("focus", onFocus)

		return () => {
			if (this.intervalTimer) clearInterval(this.intervalTimer)
			window.removeEventListener("focus", onFocus)
		}
	}
}

export const updater = new UpdaterStore()
