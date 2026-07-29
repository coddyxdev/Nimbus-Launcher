<script lang="ts">
	import { listen } from "@tauri-apps/api/event"
	import { save } from "@tauri-apps/plugin-dialog"
	import { onMount } from "svelte"
	import {
		fmtEta,
		fmtSpeed,
		installState,
		LAUNCH_STAGE_LABELS,
		STAGE_LABELS,
	} from "$lib/install.svelte"
	import {
		ipc,
		isInstalled,
		type Bootstrap,
		type Config,
		type GameExit,
		type GameOutput,
		type Instance,
		type NimbusError,
	} from "$lib/ipc"
	import { sound } from "$lib/sound.svelte"
	import { applyAccent, applyTheme, readAccent, watchSystemTheme } from "$lib/theme"
	import { toasts } from "$lib/toast.svelte"
	import { checkForUpdate, installPendingUpdate, type UpdateInfo } from "$lib/updater"
	import CommandPalette from "./CommandPalette.svelte"
	import CreateInstance from "./CreateInstance.svelte"
	import EmptyState from "./EmptyState.svelte"
	import GameConsole from "./GameConsole.svelte"
	import Header from "./Header.svelte"
	import Icon from "./Icon.svelte"
	import InstancePane from "./InstancePane.svelte"
	import Onboarding from "./Onboarding.svelte"
	import Rail, { type RailAction } from "./Rail.svelte"
	import SettingsPane from "./SettingsPane.svelte"
	import Titlebar from "./Titlebar.svelte"
	import ToastHost from "./ToastHost.svelte"

	type Phase =
		| { kind: "loading" }
		| { kind: "failed"; message: string }
		| { kind: "onboarding"; boot: Bootstrap }
		| { kind: "ready"; boot: Bootstrap }

	type ConsoleEntry = { line: string; stream: "out" | "err" }

	/** Hard cap on retained console lines per instance. */
	const MAX_CONSOLE_LINES = 5000
	/** How often buffered game output is flushed into reactive state. */
	const CONSOLE_FLUSH_MS = 100
	const MAX_DEV_LOGS = 500

	let phase = $state<Phase>({ kind: "loading" })
	let config = $state<Config | null>(null)
	let instances = $state<Instance[]>([])
	let selectedId = $state<string | null>(null)
	let view = $state<"instance" | "settings" | "create">("instance")

	let launchStates = $state<Record<string, "idle" | "starting" | "running">>({})
	/** Errors are per instance so switching builds does not show a stale message. */
	let launchErrors = $state<Record<string, string>>({})

	let consoleLines = $state<Record<string, ConsoleEntry[]>>({})
	let consoleVisible = $state(false)
	let confirmDelete = $state(false)

	let editingName = $state(false)
	let newName = $state("")

	let devMode = $state(false)
	let devLogs = $state<string[]>([])

	let updateInfo = $state<UpdateInfo | null>(null)
	let updating = $state(false)

	let paletteOpen = $state(false)

	/** Pre-launch preparation progress, keyed by instance. */
	type LaunchStage = { stage: string; done: number; total: number }
	let launchStages = $state<Record<string, LaunchStage>>({})

	/** Last abnormal exit, surfaced as a dismissible report card. */
	type CrashInfo = { instanceId: string; code: number; lines: string[] }
	let crash = $state<CrashInfo | null>(null)

	const selected = $derived(instances.find((i) => i.id === selectedId) ?? null)
	const currentConsole = $derived(selectedId ? (consoleLines[selectedId] ?? []) : [])
	const currentError = $derived(selectedId ? (launchErrors[selectedId] ?? null) : null)
	const currentState = $derived(selectedId ? (launchStates[selectedId] ?? "idle") : "idle")
	/** Ids with a live process, for the running dot in the rail. */
	const runningIds = $derived(
		Object.entries(launchStates)
			.filter(([, s]) => s !== "idle")
			.map(([id]) => id),
	)
	/** Builds whose files are incomplete, for the warning dot in the rail. */
	const brokenIds = $derived(instances.filter((i) => !isInstalled(i)).map((i) => i.id))

	function nameOf(id: string): string {
		return instances.find((i) => i.id === id)?.name ?? id
	}

	function msgOf(err: unknown): string {
		return (err as NimbusError).message ?? String(err)
	}

	function setError(id: string, message: string) {
		launchErrors = { ...launchErrors, [id]: message }
		toasts.error(message)
	}

	function clearError(id: string) {
		if (!(id in launchErrors)) return
		const next = { ...launchErrors }
		delete next[id]
		launchErrors = next
	}

	/** Drops every retained console line for an instance. */
	function forgetConsole(id: string) {
		pending.delete(id)
		if (!(id in consoleLines)) return
		const next = { ...consoleLines }
		delete next[id]
		consoleLines = next
	}

	function addDevLog(line: string) {
		const stamped = `${new Date().toLocaleTimeString("ru-RU")} ${line}`
		devLogs = [...devLogs, stamped].slice(-MAX_DEV_LOGS)
	}

	// ── Game output batching ──────────────────────
	// Minecraft emits thousands of lines per second at startup. Buffering them
	// outside of reactive state and flushing on a timer keeps the UI responsive.
	const pending = new Map<string, ConsoleEntry[]>()
	let flushTimer: ReturnType<typeof setTimeout> | null = null

	function flushConsole() {
		flushTimer = null
		if (pending.size === 0) return
		const next = { ...consoleLines }
		for (const [id, entries] of pending) {
			const merged = (next[id] ?? []).concat(entries)
			next[id] =
				merged.length > MAX_CONSOLE_LINES
					? merged.slice(merged.length - MAX_CONSOLE_LINES)
					: merged
		}
		pending.clear()
		consoleLines = next
	}

	function queueLine(payload: GameOutput) {
		const bucket = pending.get(payload.instanceId)
		if (bucket) {
			bucket.push({ line: payload.line, stream: payload.stream })
		} else {
			pending.set(payload.instanceId, [{ line: payload.line, stream: payload.stream }])
		}
		if (flushTimer === null) {
			flushTimer = setTimeout(flushConsole, CONSOLE_FLUSH_MS)
		}
	}

	async function refreshInstances() {
		try {
			instances = await ipc.listInstances()
			if (selectedId && !instances.some((i) => i.id === selectedId)) {
				selectedId = instances[0]?.id ?? null
			}
			if (!selectedId) selectedId = instances[0]?.id ?? null
		} catch (err) {
			addDevLog(`list_instances failed: ${msgOf(err)}`)
			toasts.error(`Не удалось обновить список сборок: ${msgOf(err)}`)
		}
	}

	async function boot() {
		try {
			const data = await ipc.bootstrap()
			config = data.config
			applyTheme(data.config.theme)
			if (!data.config.onboardingDone) {
				phase = { kind: "onboarding", boot: data }
				return
			}
			phase = { kind: "ready", boot: data }
			await refreshInstances()
			// Background check: never blocks boot. A misconfigured updater is
			// logged rather than swallowed, because otherwise it fails forever
			// without a single trace.
			void checkForUpdate().then((result) => {
				switch (result.status) {
					case "available":
						updateInfo = result.info
						addDevLog(`updater: доступна версия ${result.info.version}`)
						break
					case "unconfigured":
						addDevLog(
							"updater: не настроен — задайте endpoints и pubkey в tauri.conf.json",
						)
						break
					case "failed":
						addDevLog(`updater: проверка не удалась — ${result.message}`)
						break
					case "current":
						addDevLog("updater: установлена последняя версия")
						break
				}
			})
		} catch (err) {
			phase = { kind: "failed", message: msgOf(err) }
		}
	}

	onMount(() => {
		applyAccent(readAccent())
		void boot()

		const stopThemeWatch = watchSystemTheme(() => config?.theme ?? "dark")

		const unlistenOutput = listen<GameOutput>("game:output", (ev) => {
			queueLine(ev.payload)
		})

		const unlistenExit = listen<GameExit>("game:exit", (ev) => {
			const { instanceId, code, killedByUser } = ev.payload
			launchStates = { ...launchStates, [instanceId]: "idle" }
			addDevLog(`game:exit ${instanceId} code=${code} killedByUser=${killedByUser}`)
			// Stopping the game from the UI is not a failure.
			if (killedByUser) {
				toasts.info(`Игра остановлена: ${nameOf(instanceId)}`)
			} else if (code !== 0) {
				setError(
					instanceId,
					`Игра завершилась с ошибкой (код: ${code}). Смотрите консоль и краш-репорты.`,
				)
				consoleVisible = true
				// The tail of stderr is what actually explains a crash, so it is
				// lifted out of the console into a dedicated card.
				const errLines = (consoleLines[instanceId] ?? [])
					.filter((entry) => entry.stream === "err")
					.slice(-12)
					.map((entry) => entry.line)
				crash = { instanceId, code, lines: errLines }
			}
			// The preparation strip belongs to a launch that is now over.
			if (instanceId in launchStages) {
				const next = { ...launchStages }
				delete next[instanceId]
				launchStages = next
			}
			void refreshInstances()
		})

		// Pre-launch preparation (Forge processors, native extraction) can take
		// tens of seconds; without this the UI looks frozen on "Запуск…".
		const unlistenStage = listen<LaunchStage & { instanceId: string }>(
			"launch:stage",
			(ev) => {
				const { instanceId, stage, done, total } = ev.payload
				if (stage === "done") {
					const next = { ...launchStages }
					delete next[instanceId]
					launchStages = next
					return
				}
				launchStages = { ...launchStages, [instanceId]: { stage, done, total } }
			},
		)

		return () => {
			stopThemeWatch()
			if (flushTimer !== null) clearTimeout(flushTimer)
			void unlistenOutput.then((off) => off())
			void unlistenExit.then((off) => off())
			void unlistenStage.then((off) => off())
		}
	})

	async function play(targetId?: string) {
		const id = targetId ?? selected?.id
		if (!id) return
		clearError(id)
		consoleLines = { ...consoleLines, [id]: [] }
		pending.delete(id)
		launchStates = { ...launchStates, [id]: "starting" }
		consoleVisible = true
		try {
			const result = await ipc.launchInstance(id)
			launchStates = { ...launchStates, [id]: "running" }
			sound.play("launch")
			addDevLog(`launched ${id} pid=${result.pid}`)
		} catch (err) {
			launchStates = { ...launchStates, [id]: "idle" }
			setError(id, msgOf(err))
		}
	}

	async function stop(targetId?: string) {
		const id = targetId ?? selected?.id
		if (!id) return
		try {
			await ipc.killInstance(id)
			sound.play("stop")
		} catch (err) {
			setError(id, msgOf(err))
		}
	}

	async function duplicate(targetId: string) {
		try {
			const dup = await ipc.duplicateInstance(targetId, `${nameOf(targetId)} (копия)`)
			await refreshInstances()
			selectedId = dup.id
			view = "instance"
			toasts.success("Сборка скопирована")
		} catch (err) {
			setError(targetId, msgOf(err))
		}
	}

	/** Downloads, installs, and relaunches into the update found by checkForUpdate. */
	async function applyUpdate() {
		if (!updateInfo) return
		updating = true
		try {
			await installPendingUpdate()
		} catch (err) {
			updating = false
			toasts.error(`Не удалось установить обновление: ${msgOf(err)}`)
		}
	}

	/** Saves what the console currently shows into a user-picked text file. */
	async function exportConsole(text: string) {
		const instance = selected
		if (!instance) return
		try {
			const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-")
			const path = await save({
				defaultPath: `${instance.id}-${stamp}.log`,
				filters: [{ name: "Лог", extensions: ["log", "txt"] }],
			})
			if (!path) return
			await ipc.saveTextFile(path, text)
			toasts.success("Лог сохранён")
		} catch (err) {
			toasts.error(msgOf(err))
		}
	}

	/** Right-click menu in the rail. Works for any build, not only the open one. */
	function onRailAction(id: string, action: RailAction) {
		if (action !== "folder") {
			selectedId = id
			view = "instance"
			editingName = false
		}
		switch (action) {
			case "play":
				void play(id)
				break
			case "stop":
				void stop(id)
				break
			case "rename":
				newName = nameOf(id)
				editingName = true
				break
			case "duplicate":
				void duplicate(id)
				break
			case "folder":
				void ipc.openGameDir(id)
				break
			case "delete":
				confirmDelete = true
				break
		}
	}

	async function commitRename() {
		const instance = selected
		if (!instance) return
		const name = newName.trim()
		editingName = false
		if (!name || name === instance.name) return
		try {
			await ipc.renameInstance(instance.id, name)
			await refreshInstances()
			toasts.success(`Сборка переименована в «${name}»`)
		} catch (err) {
			setError(instance.id, msgOf(err))
		}
	}

	function onOnboarded(next: Config) {
		config = next
		applyTheme(next.theme)
		if (phase.kind === "onboarding") {
			phase = { kind: "ready", boot: { ...phase.boot, config: next } }
		}
		void refreshInstances()
	}

	async function onCreated(instance: Instance) {
		await refreshInstances()
		selectedId = instance.id
		view = "instance"
		toasts.success(`Сборка «${instance.name}» установлена`)
	}

	function onKeyDown(e: KeyboardEvent) {
		const target = e.target as HTMLElement | null
		const typing =
			target instanceof HTMLInputElement ||
			target instanceof HTMLTextAreaElement ||
			target instanceof HTMLSelectElement
		/** Enter/Space already activate a focused control; see below. */
		const onControl = target?.closest("button, a, [role='menuitem'], [role='tab']") != null

		if (e.key === "Escape") {
			if (paletteOpen) paletteOpen = false
			else if (confirmDelete) confirmDelete = false
			else if (editingName) editingName = false
			else if (consoleVisible) consoleVisible = false
			return
		}
		if (e.ctrlKey && e.key.toLowerCase() === "k") {
			e.preventDefault()
			sound.play("open")
			paletteOpen = !paletteOpen
			return
		}
		// While the palette is open it owns the keyboard.
		if (paletteOpen || typing || e.repeat) return

		if (e.key === "F5") {
			e.preventDefault()
			void refreshInstances()
		} else if (e.ctrlKey && e.key.toLowerCase() === "n") {
			e.preventDefault()
			view = "create"
		} else if (e.ctrlKey && e.key === ",") {
			e.preventDefault()
			view = "settings"
		} else if (e.ctrlKey && e.key.toLowerCase() === "l" && view === "instance" && selected) {
			e.preventDefault()
			consoleVisible = !consoleVisible
		} else if (e.key === "F2" && view === "instance" && selected) {
			e.preventDefault()
			newName = selected.name
			editingName = true
		} else if (e.key === "Delete" && view === "instance" && selected) {
			e.preventDefault()
			confirmDelete = true
		} else if (
			e.key === "Enter" &&
			view === "instance" &&
			selected &&
			currentState === "idle" &&
			// Without this the browser also "clicks" the focused button, so one
			// Enter press fired that action AND launched the game.
			!onControl
		) {
			e.preventDefault()
			void play()
		}
	}

	const headerTitle = $derived(
		view === "settings"
			? "Настройки"
			: view === "create"
				? "Новая сборка"
				: (selected?.name ?? "Nimbus Client"),
	)
	const LOADER_LABELS: Record<string, string> = {
		fabric: "Fabric",
		quilt: "Quilt",
		forge: "Forge",
		neoforge: "NeoForge",
	}

	/** Two-letter monogram used by the hero header avatar. */
	function monogram(name: string): string {
		const parts = name.trim().split(/[\s_-]+/).filter(Boolean)
		if (parts.length === 0) return "?"
		if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase()
		return (parts[0]![0]! + parts[1]![0]!).toUpperCase()
	}

	function fmtLastPlayed(ts: number | null): string {
		if (!ts) return "ещё не запускалась"
		return new Date(ts * 1000).toLocaleString("ru-RU", {
			day: "numeric",
			month: "short",
			hour: "2-digit",
			minute: "2-digit",
		})
	}

	const headerInitials = $derived(
		view === "instance" && selected ? monogram(selected.name) : "",
	)
	const headerIcon = $derived(
		view === "settings" ? "settings" : view === "create" ? "folderPlus" : "cube",
	)
	const headerChips = $derived.by(() => {
		if (view !== "instance" || !selected) return []
		const loader = selected.loader ? (LOADER_LABELS[selected.loader] ?? selected.loader) : "Vanilla"
		return [loader, selected.minecraftVersion ?? selected.versionId]
	})
	const headerMeta = $derived(
		view === "instance" && selected
			? `Запуск: ${fmtLastPlayed(selected.lastPlayed)}${
					fmtPlaytime(selected.totalPlaytimeSecs)
						? ` · в игре ${fmtPlaytime(selected.totalPlaytimeSecs)}`
						: ""
				}`
			: view === "settings"
				? "Общие параметры для всех сборок"
				: view === "create"
					? "Установка версии, загрузчика или модпака"
					: "",
	)
	/** A build with missing files must not be launchable. */
	const canPlay = $derived(selected !== null && isInstalled(selected))

	/** Preparation progress for the selected build, when a launch is starting. */
	const currentStage = $derived(selectedId ? (launchStages[selectedId] ?? null) : null)
	const currentStageLabel = $derived(
		currentStage ? (LAUNCH_STAGE_LABELS[currentStage.stage] ?? "Подготовка") : "",
	)
	const currentStagePct = $derived(
		currentStage && currentStage.total > 0
			? Math.min(100, Math.round((currentStage.done / currentStage.total) * 100))
			: 0,
	)

	/** "12 ч 30 мин" — omitted entirely for a build that was never played. */
	function fmtPlaytime(seconds: number | null | undefined): string {
		if (!seconds || seconds < 60) return ""
		const hours = Math.floor(seconds / 3600)
		const minutes = Math.floor((seconds % 3600) / 60)
		return hours > 0 ? `${hours} ч ${minutes} мин` : `${minutes} мин`
	}
	const crashInstanceName = $derived(crash ? nameOf(crash.instanceId) : "")

	// ── Global install progress ────────────────────────
	// The install runs in the backend, so the bar stays visible on every tab
	// instead of only on the «new instance» screen.
	const installing = $derived(installState.busy)
	const installPct = $derived(installState.pct)
	const installLabel = $derived.by(() => {
		const stage = installState.progress?.stage ?? ""
		const stageLabel = STAGE_LABELS[stage] ?? "Подготовка"
		const name = installState.name ?? installState.versionId ?? ""
		const file = installState.progress?.file
		return file ? `${stageLabel} · ${name} · ${file}` : `${stageLabel} · ${name}`
	})
	/** "12.3 МБ/с · ~2 мин 05 с", collapsed to "" when unknown. */
	const installRate = $derived.by(() => {
		const speed = fmtSpeed(installState.speed)
		const eta = fmtEta(installState.etaSeconds)
		return [speed, eta].filter((s) => s.length > 0).join(" · ")
	})
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="shell">
	<Titlebar subtitle={phase.kind === "ready" ? phase.boot.launcherVersion : ""} />

	{#if updateInfo}
		<div class="update-bar anim-fade-up" role="status" aria-live="polite">
			<span class="update-pip" aria-hidden="true"></span>
			<span class="update-text">
				Доступно обновление <b>{updateInfo.version}</b>
			</span>
			<button
				class="btn--sm"
				type="button"
				disabled={updating}
				onclick={() => {
					sound.play("click")
					void applyUpdate()
				}}
			>
				{updating ? "Устанавливается…" : "Обновить и перезапустить"}
			</button>
		</div>
	{/if}

	{#if phase.kind === "loading"}
		<div class="boot">
			<div class="boot-card anim-scale-in">
				<span class="boot-spinner" aria-hidden="true"></span>
				<span class="boot-text">Загрузка библиотеки…</span>
			</div>
		</div>
	{:else if phase.kind === "failed"}
		<div class="boot">
			<EmptyState
				icon="alert"
				tone="danger"
				title="Не удалось запустить лаунчер"
				body={phase.message}
				actionLabel="Повторить"
				onaction={() => void boot()}
			/>
		</div>
	{:else if phase.kind === "onboarding"}
		<Onboarding authUnavailable={phase.boot.authUnavailable} ondone={onOnboarded} />
	{:else}
		<div class="body">
			<Rail
				{instances}
				{selectedId}
				{view}
				{runningIds}
				{brokenIds}
				{installing}
				onselect={(id) => {
					selectedId = id
					view = "instance"
					editingName = false
				}}
				oncreate={() => {
					sound.play("tab")
					view = "create"
				}}
				onsettings={() => {
					sound.play("tab")
					view = "settings"
				}}
				onaction={onRailAction}
			/>

			<main class="work">
				<Header
					title={headerTitle}
					meta={headerMeta}
					initials={headerInitials}
					icon={headerIcon}
					chips={headerChips}
					status={view === "instance" ? currentState : "idle"}
				>
					{#snippet actions()}
						{#if view === "instance" && selected}
							<button
								class="btn--icon"
								type="button"
								title="Переименовать · F2"
								aria-label="Переименовать сборку"
								onclick={() => {
									sound.play("click")
									newName = selected.name
									editingName = true
								}}
							>
								<Icon name="edit" size={15} />
							</button>
							<button
								class="btn--icon"
								class:btn--icon-on={consoleVisible}
								type="button"
								title="Консоль · Ctrl+L"
								aria-label="Показать консоль"
								aria-pressed={consoleVisible}
								onclick={() => {
									sound.play("click")
									consoleVisible = !consoleVisible
								}}
							>
								<Icon name="terminal" size={15} />
							</button>
							<span class="act-sep" aria-hidden="true"></span>
							{#if currentState === "idle"}
								<button
									class="btn btn--play"
									type="button"
									title={canPlay ? "Enter" : "Сначала дозагрузите файлы сборки"}
									disabled={!canPlay}
									onclick={() => {
										sound.play("click")
										void play()
									}}
								>
									<Icon name="play" size={15} strokeWidth={2} />
									Играть
								</button>
							{:else}
								<button
									class="btn btn--danger-solid"
									type="button"
									onclick={() => {
										sound.play("click")
										void stop()
									}}
								>
									<Icon name="stop" size={14} strokeWidth={2} />
									{currentState === "starting" ? "Запуск…" : "Остановить"}
								</button>
							{/if}
						{/if}
					{/snippet}
				</Header>

				{#if installing}
					<div class="gprogress anim-fade-up" role="status" aria-live="polite">
						<div class="gprogress__row">
							<span class="gprogress__label">{installLabel}</span>
							<span class="gprogress__meta">
								{#if installRate}<span class="gprogress__rate tnum">{installRate}</span>{/if}
								<span class="gprogress__pct tnum">{installPct}%</span>
								<button
									class="btn--sm"
									type="button"
									disabled={installState.cancelling}
									onclick={() => void installState.cancel()}
								>
									{installState.cancelling ? "Отмена…" : "Отменить"}
								</button>
							</span>
						</div>
						<div
							class="gprogress__track"
							role="progressbar"
							aria-valuemin={0}
							aria-valuemax={100}
							aria-valuenow={installPct}
						>
							<div class="gprogress__bar" style="width: {installPct}%"></div>
						</div>
					</div>
				{/if}

				{#if currentStage}
					<div class="prep anim-fade-up" role="status" aria-live="polite">
						<span class="prep-spinner" aria-hidden="true"></span>
						<span class="prep-label">
							{currentStageLabel}
							{#if currentStage.total > 1}
								<span class="prep-count tnum">
									{currentStage.done}/{currentStage.total}
								</span>
							{/if}
						</span>
						<div
							class="prep-track"
							role="progressbar"
							aria-valuemin={0}
							aria-valuemax={100}
							aria-valuenow={currentStagePct}
						>
							<div class="prep-bar" style="width: {currentStagePct}%"></div>
						</div>
					</div>
				{/if}

				{#if crash}
					{@const report = crash}
					<div class="crash anim-fade-up" role="alert">
						<div class="crash-head">
							<span class="crash-glyph" aria-hidden="true">
								<Icon name="alert" size={15} />
							</span>
							<div class="crash-title">
								<span class="crash-name">
									«{crashInstanceName}» завершилась с ошибкой
								</span>
								<span class="crash-code tnum">код выхода: {report.code}</span>
							</div>
							<button
								class="btn--sm"
								type="button"
								onclick={() => void ipc.openCrashReportsDir(report.instanceId)}
							>
								Краш-репорты
							</button>
							<button
								class="btn--icon"
								type="button"
								aria-label="Скрыть отчёт об ошибке"
								onclick={() => (crash = null)}
							>
								<Icon name="close" size={14} />
							</button>
						</div>
						{#if report.lines.length > 0}
							<div class="crash-body">
								{#each report.lines as line}
									<span class="crash-line">{line}</span>
								{/each}
							</div>
						{:else}
							<p class="crash-hint">
								Игра не вывела сообщений в поток ошибок. Откройте консоль или
								краш-репорты, чтобы понять причину.
							</p>
						{/if}
					</div>
				{/if}

				<div class="canvas">
					{#if view === "create"}
						<CreateInstance oncreated={(i) => void onCreated(i)} />
					{:else if view === "settings"}
						{#if config}
							<SettingsPane {config} bind:devMode onconfig={(next) => (config = next)} />
						{/if}
					{:else if selected}
						<div class="pane">
							{#if editingName}
								<div class="rename anim-scale-in">
									<span class="rename-label">Новое имя</span>
									<!-- svelte-ignore a11y_autofocus -->
									<input
										class="input"
										type="text"
										aria-label="Новое имя сборки"
										autofocus
										maxlength="64"
										bind:value={newName}
										onkeydown={(e) => {
											if (e.key === "Enter") void commitRename()
											if (e.key === "Escape") editingName = false
										}}
									/>
									<button
										class="btn--sm btn--on"
										type="button"
										onclick={() => {
											sound.play("click")
											void commitRename()
										}}
									>
										Сохранить
									</button>
									<button
										class="btn--sm"
										type="button"
										onclick={() => {
											sound.play("click")
											editingName = false
										}}
									>
										Отмена
									</button>
								</div>
							{/if}

							<InstancePane
								instance={selected}
								error={currentError}
								bind:confirmDelete
								onclearerror={() => clearError(selected.id)}
								onerror={(m) => setError(selected.id, m)}
								ondeleted={() => {
									// Otherwise up to 5000 buffered lines per deleted
									// build stay in memory for the rest of the session.
									forgetConsole(selected.id)
									selectedId = null
									void refreshInstances()
								}}
								onduplicated={(id) => {
									selectedId = id
									void refreshInstances()
								}}
							/>
						</div>
					{:else}
						<div class="pane">
							<EmptyState
								icon="cube"
								title="Нет сборок"
								body="Создайте первую сборку, чтобы начать играть."
								actionLabel="Новая сборка"
								onaction={() => (view = "create")}
							/>
						</div>
					{/if}
				</div>

				{#if consoleVisible && view === "instance" && selected}
					<GameConsole
						lines={currentConsole}
						onexport={(text) => void exportConsole(text)}
						onclear={() => {
							const id = selected.id
							pending.delete(id)
							consoleLines = { ...consoleLines, [id]: [] }
						}}
					/>
				{/if}
			</main>
		</div>

		{#if devMode}
			<aside class="dev" aria-label="Лог разработчика">
				<div class="dev-header">
					<span class="dev-title">
						<Icon name="bug" size={12} />
						Режим разработчика
					</span>
					<button class="btn--sm" type="button" onclick={() => (devLogs = [])}>Очистить</button>
				</div>
				<div class="dev-body">
					{#each devLogs as line}
						<span class="dev-line">{line}</span>
					{/each}
				</div>
			</aside>
		{/if}
	{/if}
</div>

<CommandPalette
	bind:open={paletteOpen}
	{instances}
	{runningIds}
	onselect={(id) => {
		selectedId = id
		view = "instance"
		editingName = false
	}}
	onplay={(id) => void play(id)}
	onstop={(id) => void stop(id)}
	onfolder={(id) => void ipc.openGameDir(id)}
	oncreate={() => (view = "create")}
	onsettings={() => (view = "settings")}
/>

<ToastHost />

<style>
	.shell {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--bg-canvas);
		overflow: hidden;
	}

	.body {
		flex: 1;
		min-height: 0;
		display: flex;
	}

	.work {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		background: var(--bg-canvas);
	}

	/* Scroll owner for every view. Panes never scroll the window itself. */
	.canvas {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overflow-x: hidden;
	}

	.pane {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
		width: 100%;
		max-width: 1080px;
		margin: 0 auto;
		padding: var(--sp-6);
	}

	/* ── Boot ────────────────────────────────────────────────── */

	.boot {
		flex: 1;
		display: grid;
		place-items: center;
		padding: var(--sp-8);
	}

	.boot-card {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-card);
	}

	.boot-spinner {
		width: 15px;
		height: 15px;
		border-radius: var(--r-full);
		border: 2px solid var(--border-strong);
		border-top-color: var(--accent);
		animation: spinSlow 700ms linear infinite;
	}

	.boot-text {
		font-size: var(--fs-body);
		color: var(--text-secondary);
	}

	/* ── Update bar ──────────────────────────────────────────── */

	.update-bar {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-2) var(--sp-4);
		background: var(--accent-soft);
		border-bottom: 1px solid var(--accent-border);
		font-size: var(--fs-small);
		color: var(--text-primary);
	}

	.update-pip {
		flex: none;
		width: 7px;
		height: 7px;
		border-radius: var(--r-full);
		background: var(--accent);
		animation: pulseRing 2.2s var(--ease-out) infinite;
	}

	.update-text {
		flex: 1;
		min-width: 0;
	}

	/* ── Header action extras ────────────────────────────────── */

	.act-sep {
		width: 1px;
		height: 22px;
		margin: 0 var(--sp-1);
		background: var(--border);
	}

	.btn--icon-on {
		color: var(--accent);
		background: var(--accent-soft);
		box-shadow: inset 0 0 0 1px var(--accent-border);
	}

	/* ── Global install progress ─────────────────────────────── */

	.gprogress {
		flex: none;
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		padding: var(--sp-3) var(--sp-6);
		background: var(--bg-surface);
		border-bottom: 1px solid var(--border-subtle);
	}

	.gprogress__row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
	}

	.gprogress__label {
		min-width: 0;
		font-size: var(--fs-small);
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.gprogress__meta {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		font-size: var(--fs-small);
	}

	.gprogress__rate {
		color: var(--text-tertiary);
	}

	.gprogress__pct {
		font-weight: var(--fw-semibold);
		color: var(--text-primary);
	}

	.gprogress__track {
		position: relative;
		height: 4px;
		border-radius: var(--r-full);
		background: var(--bg-active);
		overflow: hidden;
	}

	.gprogress__bar {
		height: 100%;
		border-radius: var(--r-full);
		background: var(--accent);
		box-shadow: 0 0 12px -2px var(--accent-glow);
		transition: width var(--dur-base) var(--ease-out);
	}

	/* ── Launch preparation ──────────────────────────────────── */

	.prep {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-2) var(--sp-6);
		background: var(--bg-surface);
		border-bottom: 1px solid var(--border-subtle);
		font-size: var(--fs-small);
		color: var(--text-secondary);
	}

	.prep-spinner {
		flex: none;
		width: 13px;
		height: 13px;
		border-radius: var(--r-full);
		border: 2px solid var(--border-strong);
		border-top-color: var(--accent);
		animation: spinSlow 700ms linear infinite;
	}

	.prep-label {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: var(--sp-2);
	}

	.prep-count {
		color: var(--text-tertiary);
	}

	.prep-track {
		flex: 1;
		height: 3px;
		border-radius: var(--r-full);
		background: var(--bg-active);
		overflow: hidden;
	}

	.prep-bar {
		height: 100%;
		border-radius: var(--r-full);
		background: var(--accent);
		transition: width var(--dur-base) var(--ease-out);
	}

	/* ── Crash report card ───────────────────────────────────── */

	.crash {
		flex: none;
		margin: var(--sp-4) var(--sp-6) 0;
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: inset 0 0 0 1px rgba(242, 85, 90, 0.3), var(--shadow-card);
		overflow: hidden;
	}

	.crash-head {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-3) var(--sp-3) var(--sp-3) var(--sp-4);
	}

	.crash-glyph {
		flex: none;
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: var(--r-sm);
		color: var(--danger);
		background: var(--danger-soft);
	}

	.crash-title {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.crash-name {
		font-size: var(--fs-body);
		font-weight: var(--fw-medium);
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.crash-code {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.crash-body {
		display: flex;
		flex-direction: column;
		max-height: 132px;
		overflow-y: auto;
		padding: var(--sp-3) var(--sp-4);
		border-top: 1px solid var(--border-subtle);
		background: var(--bg-inset);
		font-family: var(--font-mono);
		font-size: var(--fs-micro);
		line-height: 1.6;
		color: var(--danger);
		user-select: text;
		-webkit-user-select: text;
	}

	.crash-line {
		white-space: pre-wrap;
		word-break: break-all;
	}

	.crash-hint {
		padding: var(--sp-3) var(--sp-4);
		border-top: 1px solid var(--border-subtle);
		font-size: var(--fs-small);
		line-height: 1.55;
		color: var(--text-tertiary);
	}

	/* ── Rename strip ────────────────────────────────────────── */
	.rename {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-3) var(--sp-4);
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-card);
	}

	.rename-label {
		flex: none;
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-caps);
		color: var(--text-tertiary);
	}

	/* ── Developer log drawer ────────────────────────────────── */

	.dev {
		flex: none;
		max-height: 168px;
		display: flex;
		flex-direction: column;
		border-top: 1px solid var(--border);
		background: var(--bg-inset);
		font-family: var(--font-mono);
		font-size: var(--fs-micro);
	}

	.dev-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--sp-2) var(--sp-4);
		background: var(--bg-surface);
		border-bottom: 1px solid var(--border-subtle);
	}

	.dev-title {
		display: inline-flex;
		align-items: center;
		gap: var(--sp-2);
		font-family: var(--font-sans);
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-caps);
		color: var(--text-tertiary);
	}

	.dev-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: var(--sp-2) var(--sp-4);
		display: flex;
		flex-direction: column;
		gap: 1px;
		color: var(--text-secondary);
		user-select: text;
		-webkit-user-select: text;
	}

	.dev-line {
		white-space: pre-wrap;
		word-break: break-all;
	}

	@media (prefers-reduced-motion: reduce) {
		.update-pip,
		.boot-spinner {
			animation: none;
		}
	}
</style>
