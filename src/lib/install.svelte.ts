/**
 * Global install state.
 *
 * The `install:progress` listener used to live inside CreateInstance.svelte,
 * so switching tabs unmounted the component and the running download became
 * invisible (it kept going in the backend, but nothing showed it). Keeping the
 * state in a module-level store means the progress survives navigation and any
 * screen can render it.
 */
import { listen } from "@tauri-apps/api/event"

import { ipc, isCancelled, type InstallProgress } from "./ipc"

/** Sliding window used to smooth the download speed estimate. */
const SPEED_WINDOW_MS = 4000

class InstallState {
	/** Version id currently being installed, or null when idle. */
	versionId = $state<string | null>(null)
	/** Human readable name of the instance being created. */
	name = $state<string | null>(null)
	progress = $state<InstallProgress | null>(null)
	error = $state<string | null>(null)
	/** True between the cancel request and the backend acknowledging it. */
	cancelling = $state(false)
	/** Bytes per second, or null until there are two samples to compare. */
	speed = $state<number | null>(null)

	#subscribed = false
	/** [timestamp, bytesDone] samples inside the speed window. */
	#samples: Array<[number, number]> = []

	get busy(): boolean {
		return this.versionId !== null
	}

	/** Completion percentage, 0 when the backend has not reported totals yet. */
	get pct(): number {
		const p = this.progress
		if (p && p.bytesTotal && p.bytesTotal > 0) {
			return Math.round(((p.bytesDone ?? 0) / p.bytesTotal) * 100)
		}
		return p && p.total > 0 ? Math.round((p.done / p.total) * 100) : 0
	}

	/** Remaining seconds, or null when it cannot be estimated yet. */
	get etaSeconds(): number | null {
		const p = this.progress
		const speed = this.speed
		if (!p || !speed || speed <= 0) return null
		const total = p.bytesTotal ?? 0
		const done = p.bytesDone ?? 0
		if (total <= 0 || done >= total) return null
		return Math.round((total - done) / speed)
	}

	/** Subscribes to backend progress events exactly once per app session. */
	subscribe(): void {
		if (this.#subscribed) return
		this.#subscribed = true
		void listen<InstallProgress>("install:progress", (ev) => {
			this.progress = ev.payload
			this.#sample(ev.payload)
		})
	}

	#sample(p: InstallProgress): void {
		const bytes = p.bytesDone ?? null
		if (bytes === null) return
		const now = Date.now()
		this.#samples.push([now, bytes])
		while (this.#samples.length > 1) {
			const first = this.#samples[0]
			if (!first || now - first[0] <= SPEED_WINDOW_MS) break
			this.#samples.shift()
		}
		const first = this.#samples[0]
		const last = this.#samples[this.#samples.length - 1]
		if (!first || !last) return
		const dt = (last[0] - first[0]) / 1000
		const db = last[1] - first[1]
		this.speed = dt > 0.5 && db > 0 ? db / dt : this.speed
	}

	begin(versionId: string, name: string): void {
		this.versionId = versionId
		this.name = name
		this.progress = null
		this.error = null
		this.cancelling = false
		this.speed = null
		this.#samples = []
	}

	finish(error?: string | null): void {
		this.versionId = null
		this.name = null
		this.progress = null
		this.cancelling = false
		this.speed = null
		this.#samples = []
		this.error = error ?? null
	}

	/**
	 * Asks the backend to abort the running install. The `install_version`
	 * promise then rejects with a `cancelled` error, which callers should
	 * swallow via `isCancelled`.
	 */
	async cancel(): Promise<void> {
		if (!this.busy || this.cancelling) return
		this.cancelling = true
		try {
			await ipc.cancelInstall()
		} catch {
			this.cancelling = false
		}
	}

	clearError(): void {
		this.error = null
	}
}

export const installState = new InstallState()
installState.subscribe()

/** Shared helper so every screen swallows cancellations the same way. */
export function finishInstall(err?: unknown): void {
	if (err === undefined) {
		installState.finish()
		return
	}
	if (isCancelled(err)) {
		installState.finish()
		return
	}
	installState.finish((err as { message?: string }).message ?? String(err))
}

/** Stage id → Russian label, shared by every screen that shows progress. */
export const STAGE_LABELS: Record<string, string> = {
	metadata: "Метаданные",
	java: "Java",
	client: "Клиент",
	loader: "Загрузчик",
	libraries: "Библиотеки",
	assets: "Ресурсы",
	"fabric-api": "Fabric API",
	instance: "Создание сборки",
	verify: "Проверка файлов",
	"modpack-index": "Чтение модпака",
	"modpack-files": "Файлы модпака",
	overrides: "Оверрайды",
	done: "Готово",
}

/** Labels for the `launch:stage` events emitted while preparing a launch. */
export const LAUNCH_STAGE_LABELS: Record<string, string> = {
	metadata: "Чтение метаданных",
	java: "Поиск Java",
	forge: "Подготовка Forge",
	"forge-processors": "Обработчики Forge",
	natives: "Распаковка библиотек",
	done: "Запуск игры",
}

/** Formats bytes/second for the progress UI. */
export function fmtSpeed(bytesPerSecond: number | null): string {
	if (!bytesPerSecond || bytesPerSecond <= 0) return ""
	if (bytesPerSecond >= 1024 * 1024) return `${(bytesPerSecond / (1024 * 1024)).toFixed(1)} МБ/с`
	return `${(bytesPerSecond / 1024).toFixed(0)} КБ/с`
}

/** Formats an ETA in seconds as a short Russian duration. */
export function fmtEta(seconds: number | null): string {
	if (seconds === null || seconds < 0) return ""
	if (seconds < 60) return `~${seconds} с`
	const min = Math.floor(seconds / 60)
	const sec = seconds % 60
	if (min < 60) return `~${min} мин ${sec.toString().padStart(2, "0")} с`
	const hours = Math.floor(min / 60)
	return `~${hours} ч ${(min % 60).toString().padStart(2, "0")} мин`
}
