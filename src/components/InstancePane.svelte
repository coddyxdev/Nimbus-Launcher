<script lang="ts">
	import { open, save } from "@tauri-apps/plugin-dialog"
	import Icon from "./Icon.svelte"
	import { getCurrentWebview } from "@tauri-apps/api/webview"
	import {
		ipc,
		isInstalled,
		type CrashReportInfo,
		type Instance,
		type InstanceSettings,
		type ModInfo,
		type ModrinthHit,
		type NimbusError,
	} from "$lib/ipc"
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
	}: {
		instance: Instance
		error?: string | null
		confirmDelete?: boolean
		onclearerror: () => void
		onerror: (message: string) => void
		ondeleted: () => void
		onduplicated: (newId: string) => void
	} = $props()

	type Tab = "overview" | "mods" | "browse" | "logs" | "settings"

	const TABS: { id: Tab; label: string }[] = [
		{ id: "overview", label: "Обзор" },
		{ id: "mods", label: "Моды" },
		{ id: "browse", label: "Каталог" },
		{ id: "logs", label: "Логи" },
		{ id: "settings", label: "Настройки" },
	]

	let tab = $state<Tab>("overview")
	let mods = $state<ModInfo[]>([])
	let modError = $state<string | null>(null)
	let modQuery = $state("")
	let dialogEl = $state<HTMLDivElement | null>(null)
	let sizeBytes = $state<number | null>(null)
	let verifying = $state(false)
	let exporting = $state(false)
	let dragOver = $state(false)

	// Modrinth catalogue state.
	let hitQuery = $state("")
	let hits = $state<ModrinthHit[]>([])
	let searching = $state(false)
	let installingId = $state<string | null>(null)
	let browseError = $state<string | null>(null)

	// Per-instance overrides. `null` in a field means "inherit the global value".
	let memoryOverride = $state<number | null>(null)
	let jvmOverride = $state<string | null>(null)
	let aikarOverride = $state<boolean | null>(null)
	let savingSettings = $state(false)

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

	const LOADER_NAMES: Record<string, string> = {
		fabric: "Fabric",
		quilt: "Quilt",
		forge: "Forge",
		neoforge: "NeoForge",
	}

	const visibleTabs = $derived(
		instance.loader
			? TABS
			: TABS.filter((t) => t.id !== "mods" && t.id !== "browse"),
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

	async function loadMods(instanceId: string) {
		try {
			mods = await ipc.listMods(instanceId)
			modError = null
		} catch (err) {
			modError = msgOf(err)
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

	/** Copies whatever the logs tab currently shows. */
	async function copyLog() {
		const text = openReport ? reportBody : logLines.join("\n")
		if (!text) return
		try {
			await navigator.clipboard.writeText(text)
			toasts.success("Скопировано в буфер обмена")
		} catch {
			toasts.error("Не удалось скопировать")
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
				filters: [{ name: "Журнал", extensions: ["log", "txt"] }],
			})
			if (target) {
				await ipc.saveTextFile(target, text)
				toasts.success("Файл сохранён")
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
		browseError = null
		logLines = []
		crashReports = []
		openReport = null
		reportBody = ""
		logError = null
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
				filters: [{ name: "Minecraft моды", extensions: ["jar"] }],
			})
			if (!picked) return
			const list = Array.isArray(picked) ? picked : [picked]
			for (const path of list) {
				await ipc.addMod(instance.id, path)
			}
			await loadMods(instance.id)
			toasts.success(list.length === 1 ? "Мод добавлен" : `Добавлено модов: ${list.length}`)
		} catch (err) {
			modError = msgOf(err)
			toasts.error(modError)
		}
	}

	async function handleDroppedFiles(paths: string[]) {
		const jars = paths.filter((p) => p.toLowerCase().endsWith(".jar"))
		if (jars.length === 0) {
			toasts.error("Перетащите .jar файлы модов")
			return
		}
		try {
			for (const path of jars) {
				await ipc.addMod(instance.id, path)
			}
			await loadMods(instance.id)
			toasts.success(jars.length === 1 ? "Мод добавлен" : `Добавлено модов: ${jars.length}`)
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
			toasts.info(`Мод удалён: ${fileName}`)
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
		if (!q) return
		searching = true
		browseError = null
		try {
			hits = await ipc.modrinthSearch(instance.id, q, 20)
			if (hits.length === 0) browseError = "Ничего не найдено для этой версии и загрузчика"
		} catch (err) {
			browseError = msgOf(err)
		} finally {
			searching = false
		}
	}

	async function installHit(hit: ModrinthHit) {
		installingId = hit.project_id
		try {
			const added = await ipc.modrinthInstall(instance.id, hit.project_id)
			await loadMods(instance.id)
			toasts.success(`Установлено: ${added.fileName}`)
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
			toasts.success("Настройки сборки сохранены")
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
			toasts.info("Используются общие настройки")
		} catch (err) {
			toasts.error(msgOf(err))
		}
	}

	async function verify() {
		verifying = true
		try {
			const checked = await ipc.verifyInstance(instance.id)
			toasts.success(`Проверено файлов: ${checked}`)
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
			toasts.success(`Сборка «${instance.name}» удалена`)
			ondeleted()
		} catch (err) {
			confirmDelete = false
			onerror(msgOf(err))
		}
	}

	async function doDuplicate() {
		try {
			const dup = await ipc.duplicateInstance(instance.id, `${instance.name} (копия)`)
			toasts.success("Сборка продублирована")
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
				filters: [{ name: "Резервная копия Nimbus", extensions: ["zip"] }],
			})
			if (!path) return
			exporting = true
			await ipc.exportInstance(instance.id, path)
			toasts.success("Сборка экспортирована")
		} catch (err) {
			onerror(msgOf(err))
		} finally {
			exporting = false
		}
	}

	function fmtSize(bytes: number): string {
		if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} ГБ`
		if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`
		if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} КБ`
		return `${bytes} Б`
	}

	function fmtDownloads(n: number): string {
		if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)} млн`
		if (n >= 1_000) return `${(n / 1_000).toFixed(0)} тыс`
		return String(n)
	}

	function fmtTime(ts: number | null): string {
		if (!ts) return "никогда"
		return new Date(ts * 1000).toLocaleDateString("ru-RU", {
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
		<button class="btn--sm" type="button" onclick={onclearerror}>Скрыть</button>
	</div>
{/if}

{#if !installed}
	<div class="alert alert--warn" role="status">
		<span class="alert-icon" aria-hidden="true"><Icon name="download" size={14} /></span>
		<span class="alert-text">Сборка установлена не полностью — запуск недоступен.</span>
		<button class="btn--sm" type="button" disabled={verifying} onclick={() => void verify()}>
			{verifying ? "Проверка…" : "Дозагрузить файлы"}
		</button>
	</div>
{/if}

<div class="segmented" role="tablist" aria-label="Разделы сборки">
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
				<span class="stat-label">Версия</span>
				<span class="stat-value">{instance.versionId}</span>
			</div>
			{#if instance.loader}
				<div class="stat">
					<span class="stat-label">Загрузчик</span>
					<span class="stat-value">
						<span
							class="loader-badge"
							class:badge--fabric={instance.loader === "fabric"}
							class:badge--quilt={instance.loader === "quilt"}
							class:badge--forge={instance.loader === "forge"}
							class:badge--neoforge={instance.loader === "neoforge"}
						>
							{LOADER_NAMES[instance.loader] ?? instance.loader}
						</span>
						<span class="stat-dim">{instance.loaderVersion}</span>
					</span>
				</div>
			{/if}
			<div class="stat">
				<span class="stat-label">Размер сборки</span>
				<span class="stat-value tnum">
					{sizeBytes === null ? "подсчёт…" : fmtSize(sizeBytes)}
				</span>
			</div>
			<div class="stat">
				<span class="stat-label">Создана</span>
				<span class="stat-value stat-value--sm tnum">{fmtTime(instance.createdAt)}</span>
			</div>
			<div class="stat">
				<span class="stat-label">Последний запуск</span>
				<span class="stat-value stat-value--sm tnum">{fmtTime(instance.lastPlayed)}</span>
			</div>
			{#if instance.loader}
				<div class="stat">
					<span class="stat-label">Моды</span>
					<span class="stat-value tnum">
						{mods.length}
						{#if mods.length > 0}<span class="stat-dim">{fmtSize(totalSize)}</span>{/if}
					</span>
				</div>
			{/if}
		</div>

		<div class="card">
			<div class="card__head">
				<span class="card__title">Папки и файлы</span>
			</div>
			<div class="card__body">
				<div class="tiles">
					<button class="tile-btn" type="button" onclick={() => void ipc.openGameDir(instance.id)}>
						<Icon name="folder" size={16} />
						Папка игры
					</button>
					{#if instance.loader}
						<button class="tile-btn" type="button" onclick={() => void ipc.openModsDir(instance.id)}>
							<Icon name="package" size={16} />
							Папка модов
						</button>
					{/if}
					<button class="tile-btn" type="button" onclick={() => void ipc.openScreenshotsDir(instance.id)}>
						<Icon name="image" size={16} />
						Скриншоты
					</button>
					<button class="tile-btn" type="button" onclick={() => void ipc.openLogsDir(instance.id)}>
						<Icon name="fileText" size={16} />
						Логи
					</button>
					<button class="tile-btn" type="button" onclick={() => void ipc.openCrashReportsDir(instance.id)}>
						<Icon name="bug" size={16} />
						Краш-репорты
					</button>
				</div>
			</div>
		</div>

		<div class="card">
			<div class="card__head">
				<span class="card__title">Обслуживание сборки</span>
			</div>
			<div class="card__body">
				<div class="row-actions">
					<button class="btn--sm" type="button" disabled={verifying} onclick={() => void verify()}>
						<Icon name="shieldCheck" size={14} />
						{verifying ? "Проверка…" : "Проверить файлы"}
					</button>
					<button class="btn--sm" type="button" onclick={() => void doDuplicate()}>
						<Icon name="copy" size={14} />
						Дублировать
					</button>
					<button class="btn--sm" type="button" disabled={exporting} onclick={() => void doExport()}>
						<Icon name="upload" size={14} />
						{exporting ? "Экспорт…" : "Экспорт (.zip)"}
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
						Удалить сборку
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
			aria-label="Удалить сборку"
			bind:this={dialogEl}
		>
			<span class="dialog-icon" aria-hidden="true">
				<Icon name="trash" size={18} strokeWidth={1.8} />
			</span>
			<p class="dialog-title">Удалить сборку?</p>
			<p class="dialog-body">
				Файлы сборки «{instance.name}» будут удалены безвозвратно вместе с модами,
				мирами и настройками.
			</p>
			<p class="dialog-meta tnum">
				{#if mods.length > 0}Модов: {mods.length} · {fmtSize(totalSize)}{/if}
				{#if sizeBytes !== null} · всего {fmtSize(sizeBytes)}{/if}
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
					Отмена
				</button>
				<button class="btn btn--danger-solid" type="button" onclick={() => void doDelete()}>
					Удалить навсегда
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
				Отпустите, чтобы добавить .jar в моды
			</div>
		{/if}
		<div class="card__head">
			<span class="card__title">
				Моды
				<span class="count tnum">
					{filteredMods.length}{filteredMods.length !== mods.length ? ` из ${mods.length}` : ""}
				</span>
				{#if mods.length > 0}<span class="count count--dim tnum">{fmtSize(totalSize)}</span>{/if}
			</span>
			<div class="head-tools">
				<div class="mini-search">
					<span class="mini-search-icon" aria-hidden="true"><Icon name="search" size={12} /></span>
					<input
						class="mini-search-input"
						type="text"
						placeholder="Поиск мода"
						aria-label="Поиск мода"
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
					Каталог
				</button>
				<button class="btn--sm btn--on" type="button" onclick={() => void addMod()}>
					<Icon name="plus" size={14} strokeWidth={2} />
					Добавить
				</button>
			</div>
		</div>

		{#if modError}
			<div class="inline-error" role="alert">{modError}</div>
		{/if}

		{#if mods.length === 0}
			<div class="void">
				<span class="void-glyph" aria-hidden="true"><Icon name="package" size={20} /></span>
				<span class="void-title">Модов пока нет</span>
				<span class="void-body">
					Перетащите .jar файлы в окно, добавьте их вручную или откройте каталог Modrinth.
				</span>
			</div>
		{:else if filteredMods.length === 0}
			<div class="void">
				<span class="void-title">Ничего не найдено по запросу «{modQuery}»</span>
				<button class="btn--sm" type="button" onclick={() => (modQuery = "")}>Очистить</button>
			</div>
		{:else}
			<div class="rows">
				{#each filteredMods as m (m.fileName)}
					<div class="mod" class:mod--off={!m.enabled}>
						<span class="mod-dot" class:mod-dot--off={!m.enabled} aria-hidden="true"></span>
						<div class="mod-info">
							<span class="mod-name">{m.fileName}</span>
							<span class="mod-meta tnum">
								{fmtSize(m.sizeBytes)}{m.enabled ? "" : " · отключён"}
							</span>
						</div>
						<div class="mod-actions">
							<button
								class="btn--sm"
								type="button"
								aria-pressed={m.enabled}
								aria-label={`${m.enabled ? "Отключить" : "Включить"} мод ${m.fileName}`}
								onclick={() => void toggleMod(m)}
							>
								{m.enabled ? "Отключить" : "Включить"}
							</button>
							<button
								class="btn--sm btn--danger"
								type="button"
								aria-label={`Удалить мод ${m.fileName}`}
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
			<span class="card__title">Каталог Modrinth</span>
			<div class="head-tools">
				<div class="mini-search">
					<span class="mini-search-icon" aria-hidden="true"><Icon name="search" size={12} /></span>
					<input
						class="mini-search-input"
						type="text"
						placeholder="Название мода"
						aria-label="Поиск в Modrinth"
						bind:value={hitQuery}
						onkeydown={(e) => {
							if (e.key === "Enter") void searchModrinth()
						}}
					/>
				</div>
				<button class="btn--sm btn--on" type="button" disabled={searching} onclick={() => void searchModrinth()}>
					{searching ? "Поиск…" : "Найти"}
				</button>
			</div>
		</div>

		<p class="hint">
			Результаты отфильтрованы по {LOADER_NAMES[instance.loader] ?? instance.loader}
			и {instance.minecraftVersion ?? instance.versionId}.
		</p>

		{#if browseError}
			<div class="inline-error" role="alert">{browseError}</div>
		{/if}

		{#if hits.length > 0}
			<div class="rows">
				{#each hits as h (h.project_id)}
					<div class="mod">
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
								{fmtDownloads(h.downloads)} загрузок{h.author ? ` · ${h.author}` : ""}
							</span>
						</div>
						<button
							class="btn--sm"
							type="button"
							disabled={installingId !== null}
							onclick={() => void installHit(h)}
						>
							{installingId === h.project_id ? "Установка…" : "Установить"}
						</button>
					</div>
				{/each}
			</div>
		{:else if !searching && !browseError}
			<div class="void">
				<span class="void-glyph" aria-hidden="true"><Icon name="search" size={20} /></span>
				<span class="void-title">Введите название мода</span>
				<span class="void-body">Нажмите Enter, чтобы найти совместимые проекты.</span>
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
							← Назад
						</button>
						<span class="report-name">{openReport}</span>
					</span>
					<div class="head-tools">
						<button class="btn--sm" type="button" onclick={() => void copyLog()}>
							<Icon name="copy" size={13} />
							Копировать
						</button>
						<button
							class="btn--sm"
							type="button"
							disabled={logExporting}
							onclick={() => void exportLog()}
						>
							<Icon name="download" size={13} />
							Сохранить
						</button>
					</div>
				</div>
				{#if reportLoading}
					<div class="void"><span class="void-title">Чтение отчёта…</span></div>
				{:else}
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
							{logLoading ? "Чтение…" : "Обновить"}
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
						<span class="void-title">Лог пока пуст</span>
						<span class="void-body">
							Файл появится после первого запуска этой сборки.
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
							Папка
						</button>
					</div>
				</div>

				{#if crashReports.length === 0}
					<div class="void">
						<span class="void-glyph" aria-hidden="true"><Icon name="shieldCheck" size={20} /></span>
						<span class="void-title">Краш-репортов нет</span>
						<span class="void-body">Сборка ещё ни разу не падала — так и должно быть.</span>
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

{#if tab === "settings"}
	<section class="card anim-fade-up" role="tabpanel">
		<div class="card__head">
			<span class="card__title">Параметры сборки</span>
		</div>
		<div class="card__body form">
			<p class="hint hint--flush">
				Пустые поля означают, что используются общие настройки лаунчера.
			</p>

			<label class="field">
				<span class="field-label">Память, МБ</span>
				<input
					class="input tnum"
					type="number"
					min="512"
					max="65536"
					step="512"
					placeholder="как в общих настройках"
					value={memoryOverride ?? ""}
					oninput={(e) => {
						const raw = (e.currentTarget as HTMLInputElement).value.trim()
						memoryOverride = raw === "" ? null : Number(raw)
					}}
				/>
			</label>

			<label class="field">
				<span class="field-label">Аргументы JVM</span>
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
				<span class="field-label">Флаги Aikar</span>
				<div class="seg-group" role="group" aria-label="Флаги Aikar">
					<button
						class="chip"
						class:chip--active={aikarOverride === null}
						type="button"
						onclick={() => {
							sound.play("toggle")
							aikarOverride = null
						}}
					>
						По умолчанию
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
						Вкл
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
						Выкл
					</button>
				</div>
			</div>

			<div class="form-actions">
				<button class="btn--sm" type="button" onclick={() => void resetSettings()}>
					Сбросить к общим
				</button>
				<button class="btn btn--play" type="button" disabled={savingSettings} onclick={() => void saveSettings()}>
					{savingSettings ? "Сохранение…" : "Сохранить"}
				</button>
			</div>
		</div>
	</section>
{/if}

<style>
	/* ── Alerts ──────────────────────────────────────────────── */

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

	/* ── Segmented tabs ──────────────────────────────────────── */

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

	/* ── Layout helpers ──────────────────────────────────────── */

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

	/* ── Stat grid ───────────────────────────────────────────── */

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

	/* ── Folder tiles ────────────────────────────────────────── */

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

	/* ── Confirm dialog ──────────────────────────────────────── */

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

	/* ── Card extras ─────────────────────────────────────────── */

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

	/* ── Mod / hit rows ──────────────────────────────────────── */

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

	/* ── Empty states inside cards ───────────────────────────── */

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

	/* ── Logs & crash reports ────────────────────────────────── */

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

	/* ── Form ────────────────────────────────────────────────── */

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
