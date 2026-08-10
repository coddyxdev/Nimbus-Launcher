<script lang="ts">
	import { open, save } from "@tauri-apps/plugin-dialog"
	import Icon from "./Icon.svelte"
	import ModDetails from "./ModDetails.svelte"
	import { getCurrentWebview } from "@tauri-apps/api/webview"
	import { convertFileSrc } from "@tauri-apps/api/core"
	import {
		ipc,
		isInstalled,
		type CrashAnalysis,
		type CrashReportInfo,
		type Instance,
		type CrashFinding,
		type MemoryAdvice,
		type InstanceSettings,
		type ModInfo,
		type ModpackUpdateInfo,
		type ModUpdate,
		type ModrinthHit,
		type ModrinthSort,
		type NimbusError,
		type Screenshot,
		type ServerEntry,
		type ServerStatus,
	} from "$lib/ipc"
	import { locale, t, tf } from "$lib/i18n.svelte"
	import { sound } from "$lib/sound.svelte"
	import { toasts } from "$lib/toast.svelte"

	let {
		instance,
		error = null,
		confirmDelete = $bindable(false),
		onclearerror,
		onerror,
		ondeleted,
		onduplicated,
		onplayserver,
	}: {
		instance: Instance
		error?: string | null
		confirmDelete?: boolean
		onclearerror: () => void
		onerror: (message: string) => void
		ondeleted: () => void
		onduplicated: (newId: string) => void
		onplayserver: (address: string) => void
	} = $props()

	type Tab =
		| "overview"
		| "mods"
		| "browse"
		| "screenshots"
		| "servers"
		| "logs"
		| "settings"

	const TAB_IDS: Tab[] = [
		"overview",
		"mods",
		"browse",
		"screenshots",
		"servers",
		"logs",
		"settings",
	]

	/** Russian source strings; translated on render, not at module load. */
	const TAB_LABELS: Record<Tab, string> = {
		overview: "Обзор",
		mods: "Моды",
		browse: "Каталог",
		screenshots: "Скриншоты",
		servers: "Серверы",
		logs: "Логи",
		settings: "Настройки",
	}

	const tabs = $derived(TAB_IDS.map((id) => ({ id, label: t(TAB_LABELS[id]) })))

	let tab = $state<Tab>("overview")
	let mods = $state<ModInfo[]>([])
	let modError = $state<string | null>(null)
	let modQuery = $state("")
	let dialogEl = $state<HTMLDivElement | null>(null)
	let sizeBytes = $state<number | null>(null)
	let verifying = $state(false)
	let exporting = $state(false)
	let dragOver = $state(false)

	// ── Screenshots ─────────────────────────────────────────────
	let shots = $state<Screenshot[]>([])
	let shotsLoading = $state(false)
	let shotsError = $state<string | null>(null)
	/** Screenshot opened in the lightbox, or null while showing the grid. */
	let openShot = $state<Screenshot | null>(null)
	let shotBusy = $state<string | null>(null)

	// ── Multiplayer servers ──────────────────────────────────────
	let servers = $state<ServerEntry[]>([])
	let serversLoading = $state(false)
	let serversError = $state<string | null>(null)
	/** Ping results keyed by address; missing means "not pinged yet". */
	let statuses = $state<Record<string, ServerStatus>>({})
	let pinging = $state(false)
	let addingServer = $state(false)
	let newServerName = $state("")
	let newServerAddress = $state("")

	// Modrinth catalogue state.
	let hitQuery = $state("")
	let hitSort = $state<ModrinthSort>("downloads")
	let hits = $state<ModrinthHit[]>([])
	let searching = $state(false)
	let installingId = $state<string | null>(null)
	let browseError = $state<string | null>(null)
	/** Catalogue row the user opened the Modrinth-style details sheet for. */
	let openHit = $state<ModrinthHit | null>(null)

	// Per-instance overrides. `null` in a field means "inherit the global value".
	let memoryOverride = $state<number | null>(null)
	let jvmOverride = $state<string | null>(null)
	let aikarOverride = $state<boolean | null>(null)
	let savingSettings = $state(false)
	/** Known-bad mod pairs for this build, refreshed with the mod list. */
	let modConflicts = $state<CrashFinding[]>([])
	/** Result of the last memory advice run, shown under the field. */
	let memAdvice = $state<MemoryAdvice | null>(null)
	let memAdvising = $state(false)

	// ── Logs and crash reports ───────────────────────────────────────────────
	let logLines = $state<string[]>([])
	let logLoading = $state(false)
	let logError = $state<string | null>(null)
	let crashReports = $state<CrashReportInfo[]>([])
	/** File name of the opened crash report, or null while showing the list. */
	let openReport = $state<string | null>(null)
	let reportBody = $state("")
	let reportLoading = $state(false)
	let logExporting = $state(false)
	/** Heuristic analysis of the currently open crash report, if the user asked for it. */
	let crashAnalysis = $state<CrashAnalysis | null>(null)
	let analyzing = $state(false)

	// ── Modpack auto-update ─────────────────────────────────────────────
	let modpackUpdate = $state<ModpackUpdateInfo | null>(null)
	let checkingUpdate = $state(false)
	let updatingModpack = $state(false)

	const LOADER_NAMES: Record<string, string> = {
		fabric: "Fabric",
		quilt: "Quilt",
		forge: "Forge",
		neoforge: "NeoForge",
	}

	/** Resolved on render: reading the language keeps every caller reactive. */
	function loaderName(loader: string | null): string {
		const fallback = t("Vanilla")
		return loader ? (LOADER_NAMES[loader] ?? loader) : fallback
	}

	const visibleTabs = $derived(
		instance.loader
			? tabs
			: tabs.filter((entry) => entry.id !== "mods" && entry.id !== "browse"),
	)

	const filteredMods = $derived(
		modQuery.trim()
			? mods.filter((m) => m.fileName.toLowerCase().includes(modQuery.trim().toLowerCase()))
			: mods,
	)

	const totalSize = $derived(mods.reduce((sum, m) => sum + m.sizeBytes, 0))
	const installed = $derived(isInstalled(instance))

	function msgOf(err: unknown): string {
		return (err as NimbusError).message ?? String(err)
	}

	// Conflicts are advisory: a failure here must never hide the mod list.
	async function loadConflicts() {
		try {
			modConflicts = await ipc.checkModConflicts(instance.id)
		} catch {
			modConflicts = []
		}
	}

	$effect(() => {
		if (tab !== "mods") return
		void mods.length
		void loadConflicts()
	})

	/** Fills the memory field with a size that fits this machine and mod count. */
	async function adviseMemory() {
		memAdvising = true
		try {
			const advice = await ipc.recommendMemory(instance.id)
			memAdvice = advice
			memoryOverride = advice.recommendedMib
		} catch (err) {
			toasts.error(msgOf(err))
		} finally {
			memAdvising = false
		}
	}

	/** Reads servers.dat. A build that was never played simply has no list yet. */
	async function loadServers(ping = true) {
		serversLoading = true
		try {
			servers = await ipc.listServers(instance.id)
			serversError = null
			if (ping && servers.length > 0) void pingAll()
		} catch (err) {
			serversError = msgOf(err)
		} finally {
			serversLoading = false
		}
	}

	// Pings run in parallel: a dead address must not hold up the live ones.
	async function pingAll() {
		pinging = true
		try {
			const list = servers.map((entry) => entry.ip)
			const results = await Promise.all(
				list.map(async (address) => {
					try {
						return [address, await ipc.pingServer(address)] as const
					} catch {
						return null
					}
				}),
			)
			const next: Record<string, ServerStatus> = {}
			for (const item of results) {
				if (item) next[item[0]] = item[1]
			}
			statuses = next
		} finally {
			pinging = false
		}
	}

	$effect(() => {
		if (tab !== "servers") return
		void instance.id
		void loadServers()
	})

	async function addServer() {
		if (!newServerName.trim() || !newServerAddress.trim()) return
		addingServer = true
		try {
			servers = await ipc.addServer(instance.id, newServerName, newServerAddress)
			newServerName = ""
			newServerAddress = ""
			toasts.success(t("Сервер добавлен"))
			void pingAll()
		} catch (err) {
			toasts.error(msgOf(err))
		} finally {
			addingServer = false
		}
	}

	async function removeServer(entry: ServerEntry) {
		try {
			servers = await ipc.removeServer(instance.id, entry.ip)
			toasts.success(t("Сервер удалён"))
		} catch (err) {
			toasts.error(msgOf(err))
		}
	}

	/** Reads the instance's screenshots folder. An empty folder is not an error. */
	async function loadShots() {
		shotsLoading = true
		try {
			shots = await ipc.listScreenshots(instance.id)
			shotsError = null
		} catch (err) {
			shotsError = msgOf(err)
		} finally {
			shotsLoading = false
		}
	}

	// Refresh whenever the gallery is shown or the user switches builds, so a
	// shot taken during the session appears without a manual reload.
	$effect(() => {
		if (tab !== "screenshots") return
		void instance.id
		void loadShots()
	})

	/** Bytes as KB/MB, kept local so it cannot clash with other formatters. */
	function shotSize(bytes: number): string {
		const mb = bytes / (1024 * 1024)
		return mb >= 1 ? `${mb.toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`
	}

	function shotDate(unixSeconds: number): string {
		if (!unixSeconds) return "—"
		return new Date(unixSeconds * 1000).toLocaleString(locale())
	}

	/** Copies a shot anywhere the user picks — the "share" path. */
	async function saveShot(shot: Screenshot) {
		try {
			const dest = await save({
				defaultPath: shot.fileName,
				filters: [{ name: t("Изображения"), extensions: ["png", "jpg", "jpeg"] }],
			})
			if (!dest) return
			shotBusy = shot.fileName
			await ipc.copyScreenshot(instance.id, shot.fileName, dest)
			toasts.success(t("Скриншот сохранён"))
		} catch (err) {
			toasts.error(msgOf(err))
		} finally {
			shotBusy = null
		}
	}

	async function removeShot(shot: Screenshot) {
		shotBusy = shot.fileName
		try {
			await ipc.deleteScreenshot(instance.id, shot.fileName)
			if (openShot?.fileName === shot.fileName) openShot = null
			await loadShots()
			toasts.success(t("Скриншот удалён"))
		} catch (err) {
			toasts.error(msgOf(err))
		} finally {
			shotBusy = null
		}
	}

	async function loadMods(instanceId: string) {
		try {
			mods = await ipc.listMods(instanceId)
			modError = null
		} catch (err) {
			modError = msgOf(err)
		}
	}

	// ── Mod updates ────────────────────────────────────────────────────────

	let updates = $state<ModUpdate[]>([])
	let updatesChecking = $state(false)
	let updatingFile = $state<string | null>(null)

	/**
	 * Updates are keyed by the file on disk, which carries a `.disabled` suffix
	 * for switched-off mods, while `ModInfo.fileName` is always the enabled
	 * name. Strip the suffix so both sides line up.
	 */
	const updateByFile = $derived(
		new Map(updates.map((u) => [u.fileName.replace(/\.disabled$/, ""), u])),
	)

	async function checkUpdates() {
		if (updatesChecking) return
		updatesChecking = true
		try {
			updates = await ipc.checkModUpdates(instance.id)
			modError = null
			toasts.info(
				updates.length === 0
					? t("Все моды актуальны")
					: tf("Обновлений доступно: {0}", updates.length),
			)
		} catch (err) {
			modError = msgOf(err)
		} finally {
			updatesChecking = false
		}
	}

	async function updateOne(upd: ModUpdate) {
		updatingFile = upd.fileName
		try {
			await ipc.applyModUpdate(instance.id, upd.fileName, upd.latestVersionId)
			updates = updates.filter((u) => u.fileName !== upd.fileName)
			await loadMods(instance.id)
			toasts.success(tf("{0} обновлён до {1}", upd.title, upd.latestVersion))
		} catch (err) {
			modError = msgOf(err)
		} finally {
			updatingFile = null
		}
	}

	async function updateAllMods() {
		if (updatesChecking) return
		updatesChecking = true
		try {
			const report = await ipc.applyAllModUpdates(instance.id)
			await loadMods(instance.id)
			updates = await ipc.checkModUpdates(instance.id)
			toasts.success(
				report.skipped.length === 0
					? tf("Обновлено модов: {0}", report.installed.length)
					: tf("Обновлено: {0}, не удалось: {1}", report.installed.length, report.skipped.length),
			)
		} catch (err) {
			modError = msgOf(err)
		} finally {
			updatesChecking = false
		}
	}

	/** Reads the launcher-side log for the current launch. */
	async function loadLog() {
		logLoading = true
		logError = null
		try {
			logLines = await ipc.getGameLog(instance.id)
		} catch (err) {
			logError = msgOf(err)
			logLines = []
		} finally {
			logLoading = false
		}
	}

	async function loadCrashReports() {
		try {
			crashReports = await ipc.listCrashReports(instance.id)
		} catch (err) {
			logError = msgOf(err)
		}
	}

	async function showReport(fileName: string) {
		reportLoading = true
		openReport = fileName
		reportBody = ""
		crashAnalysis = null
		try {
			reportBody = await ipc.readCrashReport(instance.id, fileName)
		} catch (err) {
			reportBody = ""
			toasts.error(msgOf(err))
			openReport = null
		} finally {
			reportLoading = false
		}
	}

	/** Runs the heuristic crash analyzer over the currently open report. */
	async function analyzeReport() {
		if (!openReport) return
		analyzing = true
		try {
			crashAnalysis = await ipc.analyzeCrashReport(instance.id, openReport)
			if (crashAnalysis.findings.length === 0) {
				toasts.info(t("Известных причин не найдено"))
			}
		} catch (err) {
			toasts.error(msgOf(err))
		} finally {
			analyzing = false
		}
	}

	/** Copies whatever the logs tab currently shows. */
	async function copyLog() {
		const text = openReport ? reportBody : logLines.join("\n")
		if (!text) return
		try {
			await navigator.clipboard.writeText(text)
			toasts.success(t("Скопировано в буфер обмена"))
		} catch {
			toasts.error(t("Не удалось скопировать"))
		}
	}

	/** Saves the visible log or crash report to a file. */
	async function exportLog() {
		const text = openReport ? reportBody : logLines.join("\n")
		if (!text) return
		logExporting = true
		try {
			const target = await save({
				defaultPath: openReport ?? `${instance.name}-latest.log`,
				filters: [{ name: t("Журнал"), extensions: ["log", "txt"] }],
			})
			if (target) {
				await ipc.saveTextFile(target, text)
				toasts.success(t("Файл сохранён"))
			}
		} catch (err) {
			toasts.error(msgOf(err))
		} finally {
			logExporting = false
		}
	}

	// Loading the log is only worth doing while the tab is actually open.
	$effect(() => {
		if (tab !== "logs") return
		const id = instance.id
		void id
		void loadLog()
		void loadCrashReports()
	})

	// Auto-load the Modrinth catalogue as soon as the tab is open, sorted like
	// Modrinth itself, and re-run (debounced while typing) on query/sort changes.
	$effect(() => {
		if (tab !== "browse" || !instance.loader) return
		const id = instance.id
		const q = hitQuery
		const sort = hitSort
		void id
		void sort
		const delay = q.trim() ? 350 : 0
		const timer = setTimeout(() => {
			void searchModrinth()
		}, delay)
		return () => clearTimeout(timer)
	})

	// Reset every per-instance view when the selection changes.
	$effect(() => {
		const id = instance.id
		const settings = instance.settings ?? null
		memoryOverride = settings?.memoryMib ?? null
		jvmOverride = settings?.jvmArgs ? settings.jvmArgs.join(" ") : null
		aikarOverride = settings?.aikarFlags ?? null
		sizeBytes = null
		hits = []
		hitQuery = ""
		hitSort = "downloads"
		browseError = null
		logLines = []
		crashReports = []
		openReport = null
		reportBody = ""
		logError = null
		crashAnalysis = null
		analyzing = false
		modpackUpdate = null
		checkingUpdate = false
		updatingModpack = false
		void ipc
			.instanceSize(id)
			.then((bytes) => {
				sizeBytes = bytes
			})
			.catch(() => {
				sizeBytes = null
			})
		if (instance.loader) {
			void loadMods(id)
		} else {
			mods = []
			if (tab === "mods" || tab === "browse") tab = "overview"
		}
		if (instance.modpackSource) {
			void checkUpdate()
		}
	})

	// Keep focus inside the destructive dialog while it is open, and close on Escape.
	$effect(() => {
		if (!confirmDelete || !dialogEl) return
		const root = dialogEl
		const nodes = Array.from(
			root.querySelectorAll<HTMLElement>(
				"button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])",
			),
		)
		nodes[0]?.focus()

		function onKeyDown(e: KeyboardEvent) {
			if (e.key === "Escape") {
				e.stopPropagation()
				confirmDelete = false
				return
			}
			if (e.key !== "Tab") return
			const first = nodes[0]
			const last = nodes[nodes.length - 1]
			if (!first || !last) return
			if (e.shiftKey && document.activeElement === first) {
				e.preventDefault()
				last.focus()
			} else if (!e.shiftKey && document.activeElement === last) {
				e.preventDefault()
				first.focus()
			}
		}

		root.addEventListener("keydown", onKeyDown)
		return () => root.removeEventListener("keydown", onKeyDown)
	})

	async function addMod() {
		try {
			const picked = await open({
				multiple: true,
				filters: [{ name: t("Minecraft моды"), extensions: ["jar"] }],
			})
			if (!picked) return
			const list = Array.isArray(picked) ? picked : [picked]
			for (const path of list) {
				await ipc.addMod(instance.id, path)
			}
			await loadMods(instance.id)
			toasts.success(list.length === 1 ? t("Мод добавлен") : tf("Добавлено модов: {0}", list.length))
		} catch (err) {
			modError = msgOf(err)
			toasts.error(modError)
		}
	}

	async function handleDroppedFiles(paths: string[]) {
		const jars = paths.filter((p) => p.toLowerCase().endsWith(".jar"))
		if (jars.length === 0) {
			toasts.error(t("Перетащите .jar файлы модов"))
			return
		}
		try {
			for (const path of jars) {
				await ipc.addMod(instance.id, path)
			}
			await loadMods(instance.id)
			toasts.success(jars.length === 1 ? t("Мод добавлен") : tf("Добавлено модов: {0}", jars.length))
		} catch (err) {
			modError = msgOf(err)
			toasts.error(modError)
		}
	}

	// Native OS drag & drop of .jar files onto the mods tab.
	$effect(() => {
		let unlisten: (() => void) | undefined
		void getCurrentWebview()
			.onDragDropEvent((event) => {
				if (!instance.loader || tab !== "mods") {
					dragOver = false
					return
				}
				if (event.payload.type === "over") {
					dragOver = true
				} else if (event.payload.type === "drop") {
					dragOver = false
					void handleDroppedFiles(event.payload.paths)
				} else {
					dragOver = false
				}
			})
			.then((fn) => {
				unlisten = fn
			})
		return () => unlisten?.()
	})

	async function removeMod(fileName: string) {
		sound.play("delete")
		try {
			await ipc.removeMod(instance.id, fileName)
			await loadMods(instance.id)
			toasts.info(tf("Мод удалён: {0}", fileName))
		} catch (err) {
			modError = msgOf(err)
			toasts.error(modError)
		}
	}

	async function toggleMod(m: ModInfo) {
		sound.play("toggle")
		try {
			await ipc.setModEnabled(instance.id, m.fileName, !m.enabled)
			await loadMods(instance.id)
		} catch (err) {
			modError = msgOf(err)
			toasts.error(modError)
		}
	}

	async function searchModrinth() {
		const q = hitQuery.trim()
		searching = true
		browseError = null
		try {
			hits = await ipc.modrinthSearch(instance.id, q, 30, hitSort)
			if (hits.length === 0) browseError = t("Ничего не найдено для этой версии и загрузчика")
		} catch (err) {
			browseError = msgOf(err)
		} finally {
			searching = false
		}
	}

	async function installHit(hit: ModrinthHit, versionId: string | null = null) {
		installingId = hit.project_id
		try {
			const added = await ipc.modrinthInstall(
				instance.id,
				hit.project_id,
				versionId ?? undefined,
			)
			await loadMods(instance.id)
			toasts.success(tf("Установлено: {0}", added.fileName))
		} catch (err) {
			browseError = msgOf(err)
			toasts.error(browseError)
		} finally {
			installingId = null
		}
	}

	async function saveSettings() {
		savingSettings = true
		sound.play("click")
		try {
			const args = (jvmOverride ?? "").trim()
			const settings: InstanceSettings = {
				memoryMib: memoryOverride,
				jvmArgs: args ? args.split(/\s+/) : null,
				aikarFlags: aikarOverride,
			}
			const empty =
				settings.memoryMib == null && settings.jvmArgs == null && settings.aikarFlags == null
			await ipc.setInstanceSettings(instance.id, empty ? null : settings)
			toasts.success(t("Настройки сборки сохранены"))
		} catch (err) {
			toasts.error(msgOf(err))
		} finally {
			savingSettings = false
		}
	}

	async function resetSettings() {
		memoryOverride = null
		jvmOverride = null
		aikarOverride = null
		try {
			await ipc.setInstanceSettings(instance.id, null)
			toasts.info(t("Используются общие настройки"))
		} catch (err) {
			toasts.error(msgOf(err))
		}
	}

	async function verify() {
		verifying = true
		try {
			const checked = await ipc.verifyInstance(instance.id)
			toasts.success(tf("Проверено файлов: {0}", checked))
		} catch (err) {
			onerror(msgOf(err))
		} finally {
			verifying = false
		}
	}

	async function doDelete() {
		try {
			await ipc.deleteInstance(instance.id)
			confirmDelete = false
			sound.play("delete")
			toasts.success(tf("Сборка «{0}» удалена", instance.name))
			ondeleted()
		} catch (err) {
			confirmDelete = false
			onerror(msgOf(err))
		}
	}

	async function doDuplicate() {
		try {
			const dup = await ipc.duplicateInstance(instance.id, tf("{0} (копия)", instance.name))
			toasts.success(t("Сборка продублирована"))
			onduplicated(dup.id)
		} catch (err) {
			onerror(msgOf(err))
		}
	}

	/** Exports this instance's game files + metadata as a portable .zip the user can move to another PC. */
	async function doExport() {
		try {
			const path = await save({
				defaultPath: `${instance.name}.zip`,
				filters: [{ name: t("Резервная копия Nimbus"), extensions: ["zip"] }],
			})
			if (!path) return
			exporting = true
			await ipc.exportInstance(instance.id, path)
			toasts.success(t("Сборка экспортирована"))
		} catch (err) {
			onerror(msgOf(err))
		} finally {
			exporting = false
		}
	}

	/** Checks whether a newer Modrinth version exists for this instance's modpack. */
	async function checkUpdate() {
		if (!instance.modpackSource) {
			modpackUpdate = null
			return
		}
		checkingUpdate = true
		try {
			modpackUpdate = await ipc.checkModpackUpdate(instance.id)
		} catch (err) {
			modpackUpdate = null
			toasts.error(msgOf(err))
		} finally {
			checkingUpdate = false
		}
	}

	/** Downloads and applies the newest Modrinth version over this instance. */
	async function applyUpdate() {
		updatingModpack = true
		sound.play("click")
		try {
			await ipc.updateModpack(instance.id)
			toasts.success(t("Модпак обновлён"))
			await checkUpdate()
		} catch (err) {
			toasts.error(msgOf(err))
		} finally {
			updatingModpack = false
		}
	}

	function fmtSize(bytes: number): string {
		if (bytes >= 1024 * 1024 * 1024) return tf("{0} ГБ", (bytes / (1024 * 1024 * 1024)).toFixed(2))
		if (bytes >= 1024 * 1024) return tf("{0} МБ", (bytes / (1024 * 1024)).toFixed(1))
		if (bytes >= 1024) return tf("{0} КБ", (bytes / 1024).toFixed(0))
		return tf("{0} Б", bytes)
	}

	/** Total across every session, accumulated by the backend on each exit. */
	function fmtPlaytime(seconds: number | null | undefined): string {
		if (!seconds) return t("ещё не играли")
		if (seconds < 60) return t("меньше минуты")
		const hours = Math.floor(seconds / 3600)
		const minutes = Math.floor((seconds % 3600) / 60)
		return hours > 0 ? tf("{0} ч {1} мин", hours, minutes) : tf("{0} мин", minutes)
	}

	function fmtDownloads(n: number): string {
		if (n >= 1_000_000) return tf("{0} млн", (n / 1_000_000).toFixed(1))
		if (n >= 1_000) return tf("{0} тыс", (n / 1_000).toFixed(0))
		return String(n)
	}

	function fmtTime(ts: number | null): string {
		if (!ts) return t("никогда")
		return new Date(ts * 1000).toLocaleDateString(locale(), {
			day: "numeric",
			month: "long",
			year: "numeric",
			hour: "2-digit",
			minute: "2-digit",
		})
	}

	// Roving tabindex: arrows move between tabs, so only the active tab is tabbable.
	function onTabKeyDown(e: KeyboardEvent) {
		if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return
		e.preventDefault()
		const list = visibleTabs
		const index = list.findIndex((t) => t.id === tab)
		const next =
			e.key === "ArrowRight"
				? (index + 1) % list.length
				: (index - 1 + list.length) % list.length
		const target = list[next]
		if (!target) return
		sound.play("tab")
		tab = target.id
		const root = (e.currentTarget as HTMLElement).parentElement
		const buttons = Array.from(root?.querySelectorAll<HTMLElement>("button") ?? [])
		buttons[next]?.focus()
	}
</script>

{#if error}
	<div class="alert alert--danger anim-fade-up" role="alert">
		<span class="alert-icon" aria-hidden="true"><Icon name="alert" size={14} /></span>
		<span class="alert-text">{error}</span>
		<button class="btn--sm" type="button" onclick={onclearerror}>{t("Скрыть")}</button>
	</div>
{/if}

{#if !installed}
	<div class="alert alert--warn" role="status">
		<span class="alert-icon" aria-hidden="true"><Icon name="download" size={14} /></span>
		<span class="alert-text">{t("Сборка установлена не полностью — запуск недоступен.")}</span>
		<button class="btn--sm" type="button" disabled={verifying} onclick={() => void verify()}>
			{verifying ? t("Проверка…") : t("Дозагрузить файлы")}
		</button>
	</div>
{/if}

<div class="segmented" role="tablist" aria-label={t("Разделы сборки")}>
	{#each visibleTabs as t (t.id)}
		<button
			class="seg-btn"
			class:seg-btn--active={tab === t.id}
			type="button"
			role="tab"
			aria-selected={tab === t.id}
			tabindex={tab === t.id ? 0 : -1}
			onclick={() => {
				sound.play("tab")
				tab = t.id
			}}
			onkeydown={onTabKeyDown}
		>
			{t.label}
			{#if t.id === "mods" && mods.length > 0}
				<span class="seg-count tnum">{mods.length}</span>
			{/if}
		</button>
	{/each}
</div>

{#if tab === "overview"}
	<section class="stack anim-fade-up" role="tabpanel">
		<div class="stats">
			<div class="stat">
				<span class="stat-label">{t("Версия")}</span>
				<span class="stat-value">{instance.versionId}</span>
			</div>
			{#if instance.loader}
				<div class="stat">
					<span class="stat-label">{t("Загрузчик")}</span>
					<span class="stat-value">
						<span
							class="loader-badge"
							class:badge--fabric={instance.loader === "fabric"}
							class:badge--quilt={instance.loader === "quilt"}
							class:badge--forge={instance.loader === "forge"}
							class:badge--neoforge={instance.loader === "neoforge"}
						>
							{loaderName(instance.loader)}
						</span>
						<span class="stat-dim">{instance.loaderVersion}</span>
					</span>
				</div>
			{/if}
			<div class="stat">
				<span class="stat-label">{t("Размер сборки")}</span>
				<span class="stat-value tnum">
					{sizeBytes === null ? t("подсчёт…") : fmtSize(sizeBytes)}
				</span>
			</div>
			<div class="stat">
				<span class="stat-label">{t("Создана")}</span>
				<span class="stat-value stat-value--sm tnum">{fmtTime(instance.createdAt)}</span>
			</div>
			<div class="stat">
				<span class="stat-label">{t("Последний запуск")}</span>
				<span class="stat-value stat-value--sm tnum">{fmtTime(instance.lastPlayed)}</span>
			</div>
			<div class="stat">
				<span class="stat-label">{t("Наиграно")}</span>
				<span class="stat-value tnum">{fmtPlaytime(instance.totalPlaytimeSecs)}</span>
			</div>
			{#if instance.loader}
				<div class="stat">
					<span class="stat-label">{t("Моды")}</span>
					<span class="stat-value tnum">
						{mods.length}
						{#if mods.length > 0}<span class="stat-dim">{fmtSize(totalSize)}</span>{/if}
					</span>
				</div>
			{/if}
		</div>

		<div class="card">
			<div class="card__head">
				<span class="card__title">{t("Папки и файлы")}</span>
			</div>
			<div class="card__body">
				<div class="tiles">
					<button class="tile-btn" type="button" onclick={() => void ipc.openGameDir(instance.id)}>
						<Icon name="folder" size={16} />
						{t("Папка игры")}
					</button>
					{#if instance.loader}
						<button class="tile-btn" type="button" onclick={() => void ipc.openModsDir(instance.id)}>
							<Icon name="package" size={16} />
							{t("Папка модов")}
						</button>
					{/if}
					<button class="tile-btn" type="button" onclick={() => void ipc.openScreenshotsDir(instance.id)}>
						<Icon name="image" size={16} />
						{t("Скриншоты")}
					</button>
					<button class="tile-btn" type="button" onclick={() => void ipc.openLogsDir(instance.id)}>
						<Icon name="fileText" size={16} />
						{t("Логи")}
					</button>
					<button class="tile-btn" type="button" onclick={() => void ipc.openCrashReportsDir(instance.id)}>
						<Icon name="bug" size={16} />
						{t("Краш-репорты")}
					</button>
				</div>
			</div>
		</div>

		{#if instance.modpackSource}
			<div class="card">
				<div class="card__head">
					<span class="card__title">{t("Обновление модпака")}</span>
				</div>
				<div class="card__body">
					{#if checkingUpdate}
						<p class="hint hint--flush">{t("Проверка обновлений…")}</p>
					{:else if modpackUpdate?.hasUpdate}
						<div class="row-actions">
							<span class="update-info">
								Доступна версия «{modpackUpdate.latestVersionName}»
							</span>
							<button
								class="btn--sm btn--on"
								type="button"
								disabled={updatingModpack}
								onclick={() => void applyUpdate()}
							>
								<Icon name="download" size={14} />
								{updatingModpack ? t("Обновление…") : t("Обновить")}
							</button>
						</div>
					{:else if modpackUpdate}
						<p class="hint hint--flush">{t("Установлена последняя версия модпака.")}</p>
					{:else}
						<div class="row-actions">
							<button class="btn--sm" type="button" onclick={() => void checkUpdate()}>
								<Icon name="refresh" size={14} />
								{t("Проверить обновления")}
							</button>
						</div>
					{/if}
				</div>
			</div>
		{/if}

		<div class="card">
			<div class="card__head">
				<span class="card__title">{t("Обслуживание сборки")}</span>
			</div>
			<div class="card__body">
				<div class="row-actions">
					<button class="btn--sm" type="button" disabled={verifying} onclick={() => void verify()}>
						<Icon name="shieldCheck" size={14} />
						{verifying ? t("Проверка…") : t("Проверить файлы")}
					</button>
					<button class="btn--sm" type="button" onclick={() => void doDuplicate()}>
						<Icon name="copy" size={14} />
						{t("Дублировать")}
					</button>
					<button class="btn--sm" type="button" disabled={exporting} onclick={() => void doExport()}>
						<Icon name="upload" size={14} />
						{exporting ? t("Экспорт…") : t("Экспорт (.zip)")}
					</button>
					<span class="spacer"></span>
					<button
						class="btn--sm btn--danger"
						type="button"
						onclick={() => {
							sound.play("warn")
							confirmDelete = true
						}}
					>
						<Icon name="trash" size={14} />
						{t("Удалить сборку")}
					</button>
				</div>
			</div>
		</div>
	</section>
{/if}

{#if confirmDelete}
	<div class="scrim anim-fade-in">
		<div
			class="dialog anim-pop-in"
			role="dialog"
			aria-modal="true"
			aria-label={t("Удалить сборку")}
			bind:this={dialogEl}
		>
			<span class="dialog-icon" aria-hidden="true">
				<Icon name="trash" size={18} strokeWidth={1.8} />
			</span>
			<p class="dialog-title">{t("Удалить сборку?")}</p>
			<p class="dialog-body">
				{tf("Файлы сборки «{0}» будут удалены безвозвратно вместе с модами, мирами и настройками.", instance.name)}
			</p>
			<p class="dialog-meta tnum">
				{#if mods.length > 0}{tf("Модов: {0} · {1}", mods.length, fmtSize(totalSize))}{/if}
				{#if sizeBytes !== null}{tf(" · всего {0}", fmtSize(sizeBytes))}{/if}
			</p>
			<div class="dialog-actions">
				<button
					class="btn"
					type="button"
					onclick={() => {
						sound.play("click")
						confirmDelete = false
					}}
				>
					{t("Отмена")}
				</button>
				<button class="btn btn--danger-solid" type="button" onclick={() => void doDelete()}>
					{t("Удалить навсегда")}
				</button>
			</div>
		</div>
	</div>
{/if}

{#if instance.loader && tab === "mods"}
	<section class="card anim-fade-up" class:card--drag={dragOver} role="tabpanel">
		{#if dragOver}
			<div class="drop" aria-hidden="true">
				<Icon name="download" size={22} />
				{t("Отпустите, чтобы добавить .jar в моды")}
			</div>
		{/if}
		<div class="card__head">
			<span class="card__title">
				{t("Моды")}
				<span class="count tnum">
					{filteredMods.length}{filteredMods.length !== mods.length ? tf(" из {0}", mods.length) : ""}
				</span>
				{#if mods.length > 0}<span class="count count--dim tnum">{fmtSize(totalSize)}</span>{/if}
			</span>
			<div class="head-tools">
				<div class="mini-search">
					<span class="mini-search-icon" aria-hidden="true"><Icon name="search" size={12} /></span>
					<input
						class="mini-search-input"
						type="text"
						placeholder={t("Поиск мода")}
						aria-label={t("Поиск мода")}
						bind:value={modQuery}
					/>
				</div>
				<button
					class="btn--sm"
					type="button"
					onclick={() => {
						sound.play("tab")
						tab = "browse"
					}}
				>
					<Icon name="globe" size={14} />
					{t("Каталог")}
				</button>
				<button
					class="btn--sm"
					type="button"
					disabled={updatesChecking || mods.length === 0}
					title={t("Проверить обновления модов на Modrinth")}
					onclick={() => void checkUpdates()}
				>
					<Icon name="download" size={14} />
					{updatesChecking ? t("Проверка…") : t("Проверить обновления")}
				</button>
				{#if updates.length > 0}
					<button
						class="btn--sm btn--on"
						type="button"
						disabled={updatesChecking}
						onclick={() => void updateAllMods()}
					>
						{tf("Обновить все ({0})", updates.length)}
					</button>
				{/if}
				<button class="btn--sm btn--on" type="button" onclick={() => void addMod()}>
					<Icon name="plus" size={14} strokeWidth={2} />
					{t("Добавить")}
				</button>
			</div>
		</div>

		{#if modError}
			<div class="inline-error" role="alert">{modError}</div>
		{/if}

		{#each modConflicts as conflict (conflict.title)}
			<div class="conflict" role="note">
				<Icon name="alert" size={16} />
				<div class="conflict-text">
					<span class="conflict-title">{conflict.title}</span>
					<span class="conflict-body">{conflict.detail}</span>
					<span class="conflict-body">{conflict.suggestion}</span>
				</div>
			</div>
		{/each}

		{#if mods.length === 0}
			<div class="void">
				<span class="void-glyph" aria-hidden="true"><Icon name="package" size={20} /></span>
				<span class="void-title">{t("Модов пока нет")}</span>
				<span class="void-body">
					{t("Перетащите .jar файлы в окно, добавьте их вручную или откройте каталог Modrinth.")}
				</span>
			</div>
		{:else if filteredMods.length === 0}
			<div class="void">
				<span class="void-title">{tf("Ничего не найдено по запросу «{0}»", modQuery)}</span>
				<button class="btn--sm" type="button" onclick={() => (modQuery = "")}>{t("Очистить")}</button>
			</div>
		{:else}
			<div class="rows">
				{#each filteredMods as m (m.fileName)}
					<div class="mod" class:mod--off={!m.enabled}>
						<span class="mod-dot" class:mod-dot--off={!m.enabled} aria-hidden="true"></span>
						<div class="mod-info">
							<span class="mod-name">
								{m.fileName}
								{#if updateByFile.get(m.fileName)}
									<span class="count">{t("Обновление")}</span>
								{/if}
							</span>
							<span class="mod-meta tnum">
								{fmtSize(m.sizeBytes)}{m.enabled ? "" : t(" · отключён")}{updateByFile.get(m.fileName)
									? ` · ${updateByFile.get(m.fileName)?.currentVersion} → ${updateByFile.get(m.fileName)?.latestVersion}`
									: ""}
							</span>
						</div>
						<div class="mod-actions">
							{#if updateByFile.get(m.fileName)}
								{@const upd = updateByFile.get(m.fileName)!}
								<button
									class="btn--sm btn--on"
									type="button"
									disabled={updatingFile === upd.fileName}
									aria-label={tf("Обновить мод {0}", upd.title)}
									onclick={() => void updateOne(upd)}
								>
									{updatingFile === upd.fileName ? t("Обновление…") : t("Обновить")}
								</button>
							{/if}
							<button
								class="btn--sm"
								type="button"
								aria-pressed={m.enabled}
								aria-label={m.enabled ? tf("Отключить мод {0}", m.fileName) : tf("Включить мод {0}", m.fileName)}
								onclick={() => void toggleMod(m)}
							>
								{m.enabled ? t("Отключить") : t("Включить")}
							</button>
							<button
								class="btn--sm btn--danger"
								type="button"
								aria-label={tf("Удалить мод {0}", m.fileName)}
								onclick={() => void removeMod(m.fileName)}
							>
								<Icon name="trash" size={13} />
							</button>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</section>
{/if}

{#if instance.loader && tab === "browse"}
	<section class="card anim-fade-up" role="tabpanel">
		<div class="card__head">
			<span class="card__title">{t("Каталог Modrinth")}</span>
			<div class="head-tools">
				<div class="mini-search">
					<span class="mini-search-icon" aria-hidden="true"><Icon name="search" size={12} /></span>
					<input
						class="mini-search-input"
						type="text"
						placeholder={t("Название мода")}
						aria-label={t("Поиск в Modrinth")}
						bind:value={hitQuery}
						onkeydown={(e) => {
							if (e.key === "Enter") void searchModrinth()
						}}
					/>
				</div>
				<select class="mini-select" bind:value={hitSort} aria-label={t("Сортировка")}>
					<option value="downloads">{t("По загрузкам")}</option>
					<option value="follows">{t("По подпискам")}</option>
					<option value="newest">{t("Сначала новые")}</option>
					<option value="updated">{t("По обновлению")}</option>
					<option value="relevance">{t("По релевантности")}</option>
				</select>
				<button class="btn--sm btn--on" type="button" disabled={searching} onclick={() => void searchModrinth()}>
					{searching ? t("Поиск…") : t("Найти")}
				</button>
			</div>
		</div>

		<p class="hint">
			{tf("Результаты отфильтрованы по {0} и {1}.", loaderName(instance.loader), instance.minecraftVersion ?? instance.versionId)}
		</p>

		{#if browseError}
			<div class="inline-error" role="alert">{browseError}</div>
		{/if}

		{#if hits.length > 0}
			<div class="rows">
				{#each hits as h (h.project_id)}
					<div class="mod">
						<button
							class="mod-open"
							type="button"
							title={t("Открыть описание")}
							onclick={() => {
								sound.play("click")
								openHit = h
							}}
						>
							{#if h.icon_url}
								<img class="hit-icon" src={h.icon_url} alt="" width="36" height="36" />
							{:else}
								<div class="hit-icon hit-icon--blank" aria-hidden="true">
									<Icon name="package" size={16} />
								</div>
							{/if}
							<div class="mod-info">
								<span class="mod-name">{h.title}</span>
								<span class="hit-desc">{h.description}</span>
								<span class="mod-meta tnum">
									{tf("{0} загрузок", fmtDownloads(h.downloads))}{h.author ? ` · ${h.author}` : ""}
								</span>
							</div>
						</button>
						<button
							class="btn--sm"
							type="button"
							disabled={installingId !== null}
							onclick={() => void installHit(h)}
						>
							{installingId === h.project_id ? t("Установка…") : t("Установить")}
						</button>
					</div>
				{/each}
			</div>
		{:else if !searching && !browseError}
			<div class="void">
				<span class="void-glyph" aria-hidden="true"><Icon name="search" size={20} /></span>
				<span class="void-title">{t("Ничего не найдено")}</span>
				<span class="void-body">{t("Попробуйте другой запрос или сортировку.")}</span>
			</div>
		{/if}
	</section>
{/if}

{#if tab === "logs"}
	<section class="stack anim-fade-up" role="tabpanel">
		{#if openReport}
			<div class="card">
				<div class="card__head">
					<span class="card__title">
						<button
							class="btn--sm"
							type="button"
							onclick={() => {
								sound.play("click")
								openReport = null
								reportBody = ""
							}}
						>
							{t("← Назад")}
						</button>
						<span class="report-name">{openReport}</span>
					</span>
					<div class="head-tools">
						<button
							class="btn--sm"
							type="button"
							disabled={analyzing || reportLoading}
							onclick={() => void analyzeReport()}
						>
							<Icon name="bug" size={13} />
							{analyzing ? t("Анализ…") : t("Анализировать")}
						</button>
						<button class="btn--sm" type="button" onclick={() => void copyLog()}>
							<Icon name="copy" size={13} />
							{t("Копировать")}
						</button>
						<button
							class="btn--sm"
							type="button"
							disabled={logExporting}
							onclick={() => void exportLog()}
						>
							<Icon name="download" size={13} />
							{t("Сохранить")}
						</button>
					</div>
				</div>
				{#if reportLoading}
					<div class="void"><span class="void-title">{t("Чтение отчёта…")}</span></div>
				{:else}
					{#if crashAnalysis}
						{#if crashAnalysis.findings.length > 0}
							<div class="findings">
								{#each crashAnalysis.findings as finding, i (i)}
									<div class="finding">
										<span class="finding-title">{finding.title}</span>
										<span class="finding-detail">{finding.detail}</span>
										<span class="finding-suggestion">{finding.suggestion}</span>
									</div>
								{/each}
								{#if crashAnalysis.suspectedMods.length > 0}
									<div class="findings-mods">
										Возможно связано с модами: {crashAnalysis.suspectedMods.join(", ")}
									</div>
								{/if}
							</div>
						{:else}
							<div class="findings-empty">
								{t("Известных причин не найдено — посмотрите текст отчёта ниже.")}
							</div>
						{/if}
					{/if}
					<pre class="dump">{reportBody}</pre>
				{/if}
			</div>
		{:else}
			<div class="card">
				<div class="card__head">
					<span class="card__title">
						Последний запуск
						{#if logLines.length > 0}
							<span class="count tnum">{logLines.length} строк</span>
						{/if}
					</span>
					<div class="head-tools">
						<button class="btn--sm" type="button" disabled={logLoading} onclick={() => void loadLog()}>
							<Icon name="refresh" size={13} />
							{logLoading ? t("Чтение…") : t("Обновить")}
						</button>
						<button
							class="btn--sm"
							type="button"
							disabled={logLines.length === 0}
							onclick={() => void copyLog()}
						>
							<Icon name="copy" size={13} />
						</button>
						<button
							class="btn--sm"
							type="button"
							disabled={logLines.length === 0 || logExporting}
							onclick={() => void exportLog()}
						>
							<Icon name="download" size={13} />
						</button>
						<button class="btn--sm" type="button" onclick={() => void ipc.openLogsDir(instance.id)}>
							<Icon name="folder" size={13} />
						</button>
					</div>
				</div>

				{#if logError}
					<div class="inline-error" role="alert">{logError}</div>
				{/if}

				{#if logLines.length === 0 && !logLoading}
					<div class="void">
						<span class="void-glyph" aria-hidden="true"><Icon name="fileText" size={20} /></span>
						<span class="void-title">{t("Лог пока пуст")}</span>
						<span class="void-body">
							{t("Файл появится после первого запуска этой сборки.")}
						</span>
					</div>
				{:else}
					<div class="dump dump--scroll">
						{#each logLines as line, i (i)}
							<span class="dump-line">{line}</span>
						{/each}
					</div>
				{/if}
			</div>

			<div class="card">
				<div class="card__head">
					<span class="card__title">
						Краш-репорты
						{#if crashReports.length > 0}
							<span class="count tnum">{crashReports.length}</span>
						{/if}
					</span>
					<div class="head-tools">
						<button
							class="btn--sm"
							type="button"
							onclick={() => void ipc.openCrashReportsDir(instance.id)}
						>
							<Icon name="folder" size={13} />
							{t("Папка")}
						</button>
					</div>
				</div>

				{#if crashReports.length === 0}
					<div class="void">
						<span class="void-glyph" aria-hidden="true"><Icon name="shieldCheck" size={20} /></span>
						<span class="void-title">{t("Краш-репортов нет")}</span>
						<span class="void-body">{t("Сборка ещё ни разу не падала — так и должно быть.")}</span>
					</div>
				{:else}
					<div class="rows">
						{#each crashReports as report (report.fileName)}
							<button class="mod report-row" type="button" onclick={() => void showReport(report.fileName)}>
								<span class="mod-dot mod-dot--crash" aria-hidden="true"></span>
								<span class="mod-info">
									<span class="mod-name">{report.fileName}</span>
									<span class="mod-meta tnum">
										{fmtSize(report.sizeBytes)} · {fmtTime(report.lastModified)}
									</span>
								</span>
								<span class="report-open">
									<Icon name="chevronRight" size={14} />
								</span>
							</button>
						{/each}
					</div>
				{/if}
			</div>
		{/if}
	</section>
{/if}

{#if tab === "servers"}
	<section class="stack anim-fade-up" role="tabpanel">
		<div class="srv-head">
			<span class="srv-count tnum">
				{serversLoading ? t("Загрузка…") : tf("Серверов: {0}", servers.length)}
			</span>
			<button
				class="btn--sm"
				type="button"
				disabled={pinging || servers.length === 0}
				onclick={() => void pingAll()}
			>
				<Icon name="refresh" size={14} />
				{pinging ? t("Проверка…") : t("Обновить статус")}
			</button>
		</div>

		<div class="srv-add">
			<input
				class="input"
				type="text"
				placeholder={t("Название сервера")}
				bind:value={newServerName}
			/>
			<input
				class="input"
				type="text"
				placeholder={t("Адрес, например play.example.com")}
				bind:value={newServerAddress}
				onkeydown={(e) => {
					if (e.key === "Enter") void addServer()
				}}
			/>
			<button
				class="btn--sm"
				type="button"
				disabled={addingServer || !newServerName.trim() || !newServerAddress.trim()}
				onclick={() => void addServer()}
			>
				<Icon name="plus" size={14} />
				{t("Добавить")}
			</button>
		</div>

		{#if serversError}
			<p class="srv-empty">{serversError}</p>
		{:else if servers.length === 0 && !serversLoading}
			<div class="srv-empty">
				<Icon name="globe" size={28} />
				<p>{t("Серверов пока нет")}</p>
				<p class="srv-hint">{t("Список общий с игрой — добавьте сервер здесь или в самой игре")}</p>
			</div>
		{:else}
			<ul class="srv-list">
				{#each servers as entry (entry.ip)}
					{@const status = statuses[entry.ip]}
					<li class="srv-row">
						<span
							class="srv-dot"
							class:srv-dot--up={status?.online}
							class:srv-dot--down={status ? !status.online : false}
						></span>
						{#if status?.favicon}
							<img class="srv-icon" src={status.favicon} alt="" />
						{:else}
							<span class="srv-icon srv-icon--blank"><Icon name="globe" size={16} /></span>
						{/if}
						<span class="srv-text">
							<span class="srv-name">{entry.name || entry.ip}</span>
							<span class="srv-sub">
								{entry.ip}
								{#if status?.online}
									<span class="tnum">
										· {tf("{0} из {1}", status.players, status.maxPlayers)} · {status.latencyMs} ms
									</span>
								{:else if status}
									<span>· {t("Офлайн")}</span>
								{/if}
							</span>
							{#if status?.motd}
								<span class="srv-motd">{status.motd}</span>
							{/if}
						</span>
						<span class="srv-actions">
							<button
								class="btn--sm"
								type="button"
								disabled={!installed}
								onclick={() => onplayserver(entry.ip)}
							>
								<Icon name="play" size={13} />
								{t("Играть")}
							</button>
							<button class="btn--sm srv-del" type="button" onclick={() => void removeServer(entry)}>
								<Icon name="trash" size={13} />
							</button>
						</span>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
{/if}

{#if tab === "screenshots"}
	<section class="stack anim-fade-up" role="tabpanel">
		<div class="gal-head">
			<span class="gal-count tnum">
				{shotsLoading ? t("Загрузка…") : tf("Скриншотов: {0}", shots.length)}
			</span>
			<div class="gal-tools">
				<button class="btn--sm" type="button" onclick={() => void loadShots()}>
					<Icon name="refresh" size={14} />
					{t("Обновить")}
				</button>
				<button
					class="btn--sm"
					type="button"
					onclick={() => void ipc.openScreenshotsDir(instance.id)}
				>
					<Icon name="folder" size={14} />
					{t("Открыть папку")}
				</button>
			</div>
		</div>

		{#if shotsError}
			<p class="gal-empty">{shotsError}</p>
		{:else if shots.length === 0 && !shotsLoading}
			<div class="gal-empty">
				<Icon name="image" size={28} />
				<p>{t("Скриншотов пока нет")}</p>
				<p class="gal-hint">{t("Нажмите F2 в игре, чтобы сделать снимок")}</p>
			</div>
		{:else}
			<div class="gal-grid">
				{#each shots as shot (shot.path)}
					<div class="gal-card" class:gal-card--busy={shotBusy === shot.fileName}>
						<button
							class="gal-thumb"
							type="button"
							title={shot.fileName}
							onclick={() => (openShot = shot)}
						>
							<img src={convertFileSrc(shot.path)} alt={shot.fileName} loading="lazy" />
						</button>
						<div class="gal-meta">
							<span class="gal-name">{shot.fileName}</span>
							<span class="gal-sub tnum">{shotDate(shot.modified)} · {shotSize(shot.sizeBytes)}</span>
						</div>
						<div class="gal-row">
							<button class="btn--sm" type="button" onclick={() => void saveShot(shot)}>
								<Icon name="download" size={13} />
								{t("Сохранить")}
							</button>
							<button
								class="btn--sm gal-del"
								type="button"
								onclick={() => void removeShot(shot)}
							>
								<Icon name="trash" size={13} />
								{t("Удалить")}
							</button>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</section>
{/if}

{#if openShot}
	<div
		class="gal-light"
		role="button"
		tabindex="0"
		aria-label={t("Закрыть")}
		onclick={() => (openShot = null)}
		onkeydown={(e) => {
			if (e.key === "Escape" || e.key === "Enter") openShot = null
		}}
	>
		<img src={convertFileSrc(openShot.path)} alt={openShot.fileName} />
		<span class="gal-light-name">{openShot.fileName}</span>
	</div>
{/if}

{#if tab === "settings"}
	<section class="card anim-fade-up" role="tabpanel">
		<div class="card__head">
			<span class="card__title">{t("Параметры сборки")}</span>
		</div>
		<div class="card__body form">
			<p class="hint hint--flush">
				{t("Пустые поля означают, что используются общие настройки лаунчера.")}
			</p>

			<label class="field">
				<span class="field-label">{t("Память, МБ")}</span>
				<input
					class="input tnum"
					type="number"
					min="512"
					max="65536"
					step="512"
					placeholder={t("как в общих настройках")}
					value={memoryOverride ?? ""}
					oninput={(e) => {
						const raw = (e.currentTarget as HTMLInputElement).value.trim()
						memoryOverride = raw === "" ? null : Number(raw)
					}}
				/>
			</label>

			<div class="mem-advice">
				<button
					class="btn--sm"
					type="button"
					disabled={memAdvising}
					onclick={() => void adviseMemory()}
				>
					<Icon name="sparkles" size={13} />
					{memAdvising ? t("Подбор…") : t("Подобрать автоматически")}
				</button>
				{#if memAdvice}
					<span class="mem-note tnum">
						{tf(
							"ОЗУ {0} ГБ · модов {1} · рекомендуем {2} МБ",
							(memAdvice.systemMib / 1024).toFixed(1),
							memAdvice.modCount,
							memAdvice.recommendedMib,
						)}
					</span>
				{/if}
			</div>

			<label class="field">
				<span class="field-label">{t("Аргументы JVM")}</span>
				<input
					class="input"
					type="text"
					placeholder="-XX:+UseG1GC …"
					value={jvmOverride ?? ""}
					oninput={(e) => {
						const raw = (e.currentTarget as HTMLInputElement).value
						jvmOverride = raw.trim() === "" ? null : raw
					}}
				/>
			</label>

			<div class="field field--row">
				<span class="field-label">{t("Флаги Aikar")}</span>
				<div class="seg-group" role="group" aria-label={t("Флаги Aikar")}>
					<button
						class="chip"
						class:chip--active={aikarOverride === null}
						type="button"
						onclick={() => {
							sound.play("toggle")
							aikarOverride = null
						}}
					>
						{t("По умолчанию")}
					</button>
					<button
						class="chip"
						class:chip--active={aikarOverride === true}
						type="button"
						onclick={() => {
							sound.play("toggle")
							aikarOverride = true
						}}
					>
						{t("Вкл")}
					</button>
					<button
						class="chip"
						class:chip--active={aikarOverride === false}
						type="button"
						onclick={() => {
							sound.play("toggle")
							aikarOverride = false
						}}
					>
						{t("Выкл")}
					</button>
				</div>
			</div>

			<div class="form-actions">
				<button class="btn--sm" type="button" onclick={() => void resetSettings()}>
					{t("Сбросить к общим")}
				</button>
				<button class="btn btn--play" type="button" disabled={savingSettings} onclick={() => void saveSettings()}>
					{savingSettings ? t("Сохранение…") : t("Сохранить")}
				</button>
			</div>
		</div>
	</section>
{/if}

{#if openHit}
	<ModDetails
		projectId={openHit.project_id}
		title={openHit.title}
		instanceId={instance.id}
		installing={installingId === openHit.project_id}
		oninstall={(versionId) => {
			const hit = openHit
			if (!hit) return
			openHit = null
			void installHit(hit, versionId)
		}}
		onclose={() => (openHit = null)}
	/>
{/if}

<style>
	/* ── Mod conflicts ────────────────────────────────────────── */
	.conflict {
		display: flex;
		gap: var(--sp-3);
		padding: var(--sp-3);
		border: 1px solid var(--warn);
		border-radius: var(--r-lg);
		background: color-mix(in srgb, var(--warn) 10%, transparent);
		color: var(--text-1);
	}
	.conflict-text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.conflict-title {
		font-weight: var(--fw-semibold, 600);
	}
	.conflict-body {
		font-size: var(--fs-small);
		color: var(--text-2);
	}

	/* ── Memory advisor ───────────────────────────────────────── */
	.mem-advice {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		flex-wrap: wrap;
		margin-top: calc(-1 * var(--sp-1, 4px));
	}
	.mem-note {
		font-size: var(--fs-micro);
		color: var(--text-3, var(--text-2));
	}

	/* ── Multiplayer servers ───────────────────────────────────── */
	.srv-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-3);
	}
	.srv-count {
		color: var(--text-2);
		font-size: var(--fs-small);
	}
	.srv-add {
		display: flex;
		gap: var(--sp-2);
	}
	.srv-add .input {
		flex: 1;
		min-width: 0;
	}

	.srv-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
	}
	.srv-row {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-2) var(--sp-3);
		border: 1px solid var(--border-1);
		border-radius: var(--r-lg);
		background: var(--bg-2);
	}
	.srv-dot {
		flex: none;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--border-2, var(--border-1));
	}
	.srv-dot--up {
		background: #3ecf8e;
		box-shadow: 0 0 8px rgba(62, 207, 142, 0.6);
	}
	.srv-dot--down {
		background: var(--danger);
	}
	.srv-icon {
		flex: none;
		width: 32px;
		height: 32px;
		border-radius: var(--r-sm, 6px);
		object-fit: cover;
		image-rendering: pixelated;
	}
	.srv-icon--blank {
		display: grid;
		place-items: center;
		background: var(--bg-3);
		color: var(--text-3, var(--text-2));
	}
	.srv-text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
		flex: 1;
	}
	.srv-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.srv-sub,
	.srv-motd {
		font-size: var(--fs-micro);
		color: var(--text-3, var(--text-2));
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.srv-actions {
		display: flex;
		gap: var(--sp-2);
		flex: none;
	}
	.srv-del:hover {
		color: var(--danger);
	}
	.srv-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--sp-2);
		padding: var(--sp-6) var(--sp-4);
		color: var(--text-2);
		text-align: center;
	}
	.srv-hint {
		font-size: var(--fs-small);
		color: var(--text-3, var(--text-2));
	}

	/* ── Screenshot gallery ────────────────────────────────────── */
	.gal-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-3);
	}
	.gal-count {
		color: var(--text-2);
		font-size: var(--fs-small);
	}
	.gal-tools {
		display: flex;
		gap: var(--sp-2);
	}

	.gal-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
		gap: var(--sp-3);
	}
	.gal-card {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		padding: var(--sp-2);
		border: 1px solid var(--border-1);
		border-radius: var(--r-lg);
		background: var(--bg-2);
		transition: border-color var(--dur-2) var(--ease-out);
	}
	.gal-card:hover {
		border-color: var(--accent-soft, var(--border-2));
	}
	.gal-card--busy {
		opacity: 0.55;
		pointer-events: none;
	}

	.gal-thumb {
		padding: 0;
		border: 0;
		border-radius: var(--r-md);
		overflow: hidden;
		background: var(--bg-3);
		cursor: pointer;
		aspect-ratio: 16 / 9;
	}
	.gal-thumb img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
		transition: transform var(--dur-2) var(--ease-out);
	}
	.gal-thumb:hover img {
		transform: scale(1.04);
	}

	.gal-meta {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.gal-name {
		font-size: var(--fs-small);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.gal-sub {
		font-size: var(--fs-micro);
		color: var(--text-3, var(--text-2));
	}
	.gal-row {
		display: flex;
		gap: var(--sp-2);
	}
	.gal-row :global(button) {
		flex: 1;
	}
	.gal-del:hover {
		color: var(--danger);
	}

	.gal-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--sp-2);
		padding: var(--sp-6) var(--sp-4);
		color: var(--text-2);
		text-align: center;
	}
	.gal-hint {
		font-size: var(--fs-small);
		color: var(--text-3, var(--text-2));
	}

	.gal-light {
		position: fixed;
		inset: 0;
		z-index: 60;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: var(--sp-3);
		padding: var(--sp-6);
		background: rgba(0, 0, 0, 0.82);
		backdrop-filter: blur(6px);
		cursor: zoom-out;
	}
	.gal-light img {
		max-width: 92vw;
		max-height: 80vh;
		border-radius: var(--r-lg);
		box-shadow: var(--shadow-pop);
	}
	.gal-light-name {
		color: #fff;
		font-size: var(--fs-small);
		opacity: 0.85;
	}
	/* ── Alerts ─────────────────────────────────────────── */

	.alert {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-3) var(--sp-4);
		border-radius: var(--r-lg);
		font-size: var(--fs-small);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top);
	}

	.alert-icon {
		flex: none;
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		border-radius: var(--r-sm);
	}

	.alert-text {
		flex: 1;
		min-width: 0;
		color: var(--text-primary);
	}

	.alert--danger {
		box-shadow: inset 0 0 0 1px rgba(242, 85, 90, 0.3);
	}
	.alert--danger .alert-icon {
		color: var(--danger);
		background: var(--danger-soft);
	}

	.alert--warn {
		box-shadow: inset 0 0 0 1px rgba(226, 163, 54, 0.28);
	}
	.alert--warn .alert-icon {
		color: var(--warn);
		background: var(--warn-soft);
	}

	.inline-error {
		margin: 0 var(--sp-5) var(--sp-4);
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-sm);
		font-size: var(--fs-small);
		color: var(--danger);
		background: var(--danger-soft);
	}

	/* ── Segmented tabs ────────────────────────────────────── */

	.segmented {
		display: inline-flex;
		align-self: flex-start;
		gap: 2px;
		padding: 3px;
		border-radius: var(--r-md);
		background: var(--bg-surface);
		box-shadow: var(--edge-ring);
	}

	.seg-btn {
		display: inline-flex;
		align-items: center;
		gap: var(--sp-2);
		min-height: 28px;
		padding: 0 var(--sp-4);
		border-radius: var(--r-sm);
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-tertiary);
		transition:
			background var(--dur-fast) var(--ease-out),
			color var(--dur-fast) var(--ease-out),
			box-shadow var(--dur-fast) var(--ease-out);
	}
	.seg-btn:hover {
		color: var(--text-primary);
	}
	.seg-btn--active {
		color: var(--text-primary);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-sm);
	}

	.seg-count {
		padding: 0 5px;
		border-radius: var(--r-full);
		font-size: 10px;
		line-height: 15px;
		color: var(--text-tertiary);
		background: var(--bg-active);
	}
	.seg-btn--active .seg-count {
		color: var(--accent);
		background: var(--accent-soft);
	}

	/* ── Layout helpers ──────────────────────────────────── */

	.stack {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	.spacer {
		flex: 1;
	}

	.hint {
		padding: 0 var(--sp-5) var(--sp-4);
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}
	.hint--flush {
		padding: 0;
	}

	/* ── Stat grid ────────────────────────────────────── */

	.stats {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: var(--sp-3);
	}

	.stat {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		padding: var(--sp-4);
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top);
		transition:
			box-shadow var(--dur-base) var(--ease-out),
			transform var(--dur-base) var(--ease-out);
	}
	.stat:hover {
		transform: translateY(-1px);
		box-shadow:
			inset 0 0 0 1px var(--border-strong), var(--edge-top), var(--shadow-card);
	}

	.stat-label {
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-caps);
		color: var(--text-tertiary);
	}

	.stat-value {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		font-family: var(--font-display);
		font-size: var(--fs-title);
		font-weight: var(--fw-semibold);
		letter-spacing: var(--tracking-tighter);
		color: var(--text-primary);
	}
	.stat-value--sm {
		font-size: var(--fs-body);
		font-weight: var(--fw-medium);
		letter-spacing: var(--tracking-tight);
		color: var(--text-secondary);
	}

	.stat-dim {
		font-family: var(--font-sans);
		font-size: var(--fs-small);
		font-weight: var(--fw-regular);
		color: var(--text-tertiary);
	}

	.loader-badge {
		display: inline-flex;
		align-items: center;
		height: 20px;
		padding: 0 var(--sp-2);
		border-radius: var(--r-xs);
		font-family: var(--font-sans);
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		color: var(--text-secondary);
		background: var(--bg-hover);
	}
	.badge--fabric {
		color: #d5b071;
	}
	.badge--quilt {
		color: #b28ad8;
	}
	.badge--forge {
		color: #8fa4c4;
	}
	.badge--neoforge {
		color: #d99168;
	}

	/* ── Folder tiles ──────────────────────────────────── */

	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
		gap: var(--sp-2);
	}

	.tile-btn {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		min-height: 42px;
		padding: 0 var(--sp-3);
		border-radius: var(--r-md);
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		text-align: left;
		color: var(--text-secondary);
		background: var(--bg-surface);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
		transition:
			background var(--dur-fast) var(--ease-out),
			color var(--dur-fast) var(--ease-out),
			box-shadow var(--dur-fast) var(--ease-out),
			transform var(--dur-fast) var(--ease-spring);
	}
	.tile-btn:hover {
		color: var(--text-primary);
		background: var(--bg-hover);
		box-shadow: inset 0 0 0 1px var(--border);
		transform: translateY(-1px);
	}
	.tile-btn:active {
		transform: scale(0.985);
	}

	.row-actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--sp-2);
	}

	.update-info {
		font-size: var(--fs-small);
		color: var(--text-secondary);
	}

	/* ── Confirm dialog ────────────────────────────── */

	.scrim {
		position: fixed;
		inset: 0;
		z-index: var(--z-modal);
		display: grid;
		place-items: center;
		padding: var(--sp-6);
		background: var(--bg-scrim);
		backdrop-filter: blur(6px);
	}

	.dialog {
		width: 100%;
		max-width: 380px;
		padding: var(--sp-6);
		border-radius: var(--r-xl);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-overlay);
		text-align: left;
	}

	.dialog-icon {
		display: grid;
		place-items: center;
		width: 38px;
		height: 38px;
		margin-bottom: var(--sp-4);
		border-radius: var(--r-md);
		color: var(--danger);
		background: var(--danger-soft);
		box-shadow: inset 0 0 0 1px rgba(242, 85, 90, 0.25);
	}

	.dialog-title {
		font-family: var(--font-display);
		font-size: var(--fs-title);
		font-weight: var(--fw-semibold);
		letter-spacing: var(--tracking-tighter);
		color: var(--text-primary);
	}

	.dialog-body {
		margin-top: var(--sp-2);
		font-size: var(--fs-small);
		line-height: 1.55;
		color: var(--text-secondary);
	}

	.dialog-meta {
		margin-top: var(--sp-3);
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.dialog-actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--sp-2);
		margin-top: var(--sp-6);
	}

	/* ── Card extras ───────────────────────────────── */

	.card {
		position: relative;
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-card);
	}

	.card__head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border-subtle);
	}

	.card__title {
		display: inline-flex;
		align-items: center;
		gap: var(--sp-2);
		font-size: var(--fs-body);
		font-weight: var(--fw-semibold);
		color: var(--text-primary);
	}

	.card__body {
		padding: var(--sp-5);
	}

	.card--drag {
		box-shadow: inset 0 0 0 1px var(--accent-border), 0 0 0 3px var(--accent-soft);
	}

	.count {
		font-size: var(--fs-micro);
		font-weight: var(--fw-medium);
		color: var(--text-tertiary);
	}
	.count--dim {
		color: var(--text-disabled);
	}

	.head-tools {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
	}

	.mini-search {
		position: relative;
		display: flex;
		align-items: center;
	}

	.mini-search-icon {
		position: absolute;
		left: var(--sp-2);
		display: grid;
		place-items: center;
		color: var(--text-tertiary);
		pointer-events: none;
	}

	.mini-search-input {
		width: 150px;
		height: 28px;
		padding: 0 var(--sp-2) 0 26px;
		border: 0;
		border-radius: var(--r-sm);
		background: var(--bg-inset);
		color: var(--text-primary);
		font-size: var(--fs-small);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
		user-select: text;
		-webkit-user-select: text;
		transition:
			box-shadow var(--dur-fast) var(--ease-out),
			width var(--dur-base) var(--ease-out);
	}
	.mini-search-input::placeholder {
		color: var(--text-tertiary);
	}
	.mini-search-input:focus {
		outline: none;
		width: 190px;
		box-shadow:
			inset 0 0 0 1px var(--accent-border), 0 0 0 3px var(--accent-soft);
	}

	.mini-select {
		height: 28px;
		padding: 0 var(--sp-2);
		border: 0;
		border-radius: var(--r-sm);
		background: var(--bg-inset);
		color: var(--text-primary);
		font-size: var(--fs-small);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
	}
	.mini-select:hover {
		box-shadow: inset 0 0 0 1px var(--border-strong);
	}
	.mini-select:focus {
		outline: none;
		box-shadow:
			inset 0 0 0 1px var(--accent-border), 0 0 0 3px var(--accent-soft);
	}

	.drop {
		position: absolute;
		inset: 0;
		z-index: 2;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: var(--sp-3);
		border-radius: var(--r-lg);
		font-size: var(--fs-body);
		font-weight: var(--fw-medium);
		color: var(--accent);
		background: var(--bg-scrim);
		backdrop-filter: blur(3px);
	}

	/* ── Mod / hit rows ───────────────────────────── */

	.rows {
		display: flex;
		flex-direction: column;
		padding: var(--sp-2);
		max-height: 460px;
		overflow-y: auto;
	}

	.mod {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-md);
		transition: background var(--dur-fast) var(--ease-out);
	}
	.mod:hover {
		background: var(--bg-hover);
	}
	.mod--off {
		opacity: 0.6;
	}

	.mod-dot {
		flex: none;
		width: 6px;
		height: 6px;
		border-radius: var(--r-full);
		background: var(--accent);
	}
	.mod-dot--off {
		background: var(--text-disabled);
	}

	.mod-info {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.mod-name {
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.mod-meta {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.mod-actions {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		opacity: 0.55;
		transition: opacity var(--dur-fast) var(--ease-out);
	}
	.mod:hover .mod-actions,
	.mod:focus-within .mod-actions {
		opacity: 1;
	}

	/* Clickable part of a catalogue row: opens the Modrinth-style details sheet. */
	.mod-open {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: 0;
		border: 0;
		background: none;
		text-align: left;
		color: inherit;
		font: inherit;
		cursor: pointer;
	}

	.hit-icon {
		flex: none;
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border-radius: var(--r-sm);
		object-fit: cover;
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
		color: var(--text-tertiary);
	}

	.hit-desc {
		font-size: var(--fs-micro);
		color: var(--text-secondary);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	/* ── Empty states inside cards ─────────────────── */

	.void {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--sp-2);
		padding: var(--sp-10) var(--sp-6);
		text-align: center;
	}

	.void-glyph {
		display: grid;
		place-items: center;
		width: 44px;
		height: 44px;
		margin-bottom: var(--sp-2);
		border-radius: var(--r-lg);
		color: var(--text-tertiary);
		background: var(--bg-surface);
		box-shadow: var(--edge-ring);
	}

	.void-title {
		font-size: var(--fs-body);
		font-weight: var(--fw-semibold);
		color: var(--text-primary);
	}

	.void-body {
		max-width: 380px;
		font-size: var(--fs-small);
		line-height: 1.55;
		color: var(--text-tertiary);
	}

	/* ── Logs & crash reports ───────────────────── */

	.dump {
		margin: 0;
		padding: var(--sp-3) var(--sp-4);
		font-family: var(--font-mono);
		font-size: var(--fs-micro);
		line-height: 1.6;
		color: var(--text-secondary);
		background: var(--bg-inset);
		border-radius: 0 0 var(--r-lg) var(--r-lg);
		white-space: pre-wrap;
		word-break: break-word;
		user-select: text;
		-webkit-user-select: text;
		max-height: 420px;
		overflow: auto;
	}

	.dump--scroll {
		display: flex;
		flex-direction: column;
	}

	.dump-line {
		white-space: pre-wrap;
		word-break: break-all;
	}

	.findings {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		padding: var(--sp-3) var(--sp-4);
		border-bottom: 1px solid var(--border-subtle);
	}

	.finding {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-md);
		background: var(--bg-surface);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
	}

	.finding-title {
		font-size: var(--fs-small);
		font-weight: var(--fw-semibold);
		color: var(--text-primary);
	}

	.finding-detail {
		font-size: var(--fs-micro);
		color: var(--text-secondary);
	}

	.finding-suggestion {
		font-size: var(--fs-micro);
		color: var(--accent);
	}

	.findings-mods {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.findings-empty {
		padding: var(--sp-3) var(--sp-4);
		font-size: var(--fs-small);
		color: var(--text-tertiary);
		border-bottom: 1px solid var(--border-subtle);
	}

	.report-name {
		font-family: var(--font-mono);
		font-size: var(--fs-small);
		font-weight: var(--fw-regular);
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.report-row {
		width: 100%;
		text-align: left;
	}

	.report-open {
		flex: none;
		display: grid;
		place-items: center;
		color: var(--text-tertiary);
	}

	.mod-dot--crash {
		background: var(--danger);
	}

	/* ── Form ─────────────────────────────────────── */

	.form {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
	}
	.field--row {
		flex-direction: row;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
	}

	.field-label {
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-secondary);
	}

	.seg-group {
		display: flex;
		gap: var(--sp-2);
	}

	.form-actions {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: var(--sp-2);
		padding-top: var(--sp-2);
		border-top: 1px solid var(--border-subtle);
	}
</style>
