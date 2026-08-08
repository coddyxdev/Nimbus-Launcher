<script lang="ts">
	import { open } from "@tauri-apps/plugin-dialog"
	import Icon from "./Icon.svelte"
	import {
		finishInstall,
		fmtEta,
		fmtSpeed,
		installState,
		STAGE_LABELS,
	} from "$lib/install.svelte"
	import {
		ipc,
		type Instance,
		type LoaderVersionInfo,
		type ModLoader,
		type ModrinthHit,
		type ModrinthSort,
		type NimbusError,
		type PrismCandidate,
		type VersionSummary,
	} from "$lib/ipc"
	import { sound } from "$lib/sound.svelte"
	import EmptyState from "./EmptyState.svelte"
	import ModDetails from "./ModDetails.svelte"
	import Skeleton from "./Skeleton.svelte"

	let {
		oncreated,
	}: {
		oncreated: (instance: Instance) => void
	} = $props()

	type Phase =
		| { kind: "loading" }
		| { kind: "failed"; message: string }
		| { kind: "ready" }

	let phase = $state<Phase>({ kind: "loading" })
	let versions = $state<VersionSummary[]>([])
	let includeSnapshots = $state(false)
	let search = $state("")

	// ── Loader state ─────────────────────────────
	const LOADERS: { id: ModLoader; label: string }[] = [
		{ id: "fabric", label: "Fabric" },
		{ id: "quilt", label: "Quilt" },
		{ id: "forge", label: "Forge" },
		{ id: "neoforge", label: "NeoForge" },
	]
	let selectedLoader = $state<ModLoader | null>(null)
	let loaderVersions = $state<LoaderVersionInfo[]>([])
	let loaderPhase = $state<"idle" | "loading" | "failed">("idle")
	let selectedLoaderVersion = $state<string | null>(null)
	// Track which MC version the loader versions are loaded for
	let loaderForVersion = $state<string | null>(null)

	// ── Installation state ─────────────────
	// Installation runs in the backend and its state lives in a module-level
	// store, so leaving this tab (which unmounts the component) no longer
	// hides a running download.
	let instanceName = $state("")

	/** Marker used as the "version id" for a modpack import, since there is no version row for it. */
	const MODPACK_MARKER = "__modpack__"
	/** Marker used as the "version id" for a backup import, since there is no version row for it. */
	const BACKUP_MARKER = "__backup__"
	/** Marker used as the "version id" while a Modrinth modpack install is running. */
	const MODRINTH_MODPACK_MARKER = "__modrinth_modpack__"

	const installingId = $derived(installState.versionId)
	const progress = $derived(installState.progress)
	const installError = $derived(installState.error)
	const progressPct = $derived(installState.pct)
	/** "12.3 МБ/с · ~2 мин 05 с", empty until there is enough data. */
	const rate = $derived.by(() =>
		[fmtSpeed(installState.speed), fmtEta(installState.etaSeconds)]
			.filter((s) => s.length > 0)
			.join(" · "),
	)

	/**
	 * Normalises a version query so that sloppy input still matches.
	 * "1 20 1", "1,20,1", " 1.20.1 " and "1-20-1" all become "1.20.1".
	 * Type words are handled separately by `matchesType`.
	 */
	function normalize(input: string): string {
		return input
			.trim()
			.toLowerCase()
			.replace(/[,\s_\-–—]+/g, ".")
			.replace(/\.{2,}/g, ".")
			.replace(/^\.|\.$/g, "")
	}

	/** Lets the user type "релиз" / "release" / "снапшот" / "snapshot". */
	function matchesType(v: VersionSummary, needle: string): boolean {
		if (needle === "релиз" || needle === "release") return v.type === "release"
		if (needle === "снапшот" || needle === "snapshot") return v.type === "snapshot"
		return false
	}

	const query = $derived(normalize(search))
	const rawQuery = $derived(search.trim().toLowerCase())

	const filtered = $derived.by(() => {
		if (!query) return versions
		return versions.filter(
			(v) => normalize(v.id).includes(query) || matchesType(v, rawQuery),
		)
	})

	/**
	 * If a query finds nothing among releases, tell the user whether turning
	 * snapshots on would help instead of showing a dead end.
	 */
	const snapshotHint = $derived(
		!includeSnapshots && query.length > 0 && filtered.length === 0,
	)

	/** Release versions only, used for the Modrinth modpack version picker. */
	const releaseVersions = $derived(versions.filter((v) => v.type === "release"))

	async function load() {
		phase = { kind: "loading" }
		try {
			versions = await ipc.listVersions(includeSnapshots)
			phase = { kind: "ready" }
		} catch (err) {
			phase = {
				kind: "failed",
				message: (err as NimbusError).message ?? String(err),
			}
		}
	}

	$effect(() => {
		void includeSnapshots
		void load()
	})

	async function loadLoaderVersions(versionId: string) {
		if (!selectedLoader) {
			loaderVersions = []
			selectedLoaderVersion = null
			return
		}
		loaderPhase = "loading"
		loaderForVersion = null
		try {
			loaderVersions = await ipc.listLoaderVersions(selectedLoader, versionId)
			loaderForVersion = versionId
			// Auto-select first stable version, or first version
			const stable = loaderVersions.find((v) => v.stable)
			selectedLoaderVersion = stable?.version ?? loaderVersions[0]?.version ?? null
			loaderPhase = "idle"
		} catch {
			loaderVersions = []
			selectedLoaderVersion = null
			loaderPhase = "failed"
		}
	}

	async function ensureLoaderVersions(v: VersionSummary) {
		// If loader versions are stale (loaded for a different MC version), reload them
		if (selectedLoader && loaderForVersion !== v.id) {
			await loadLoaderVersions(v.id)
		}
	}

	async function install(v: VersionSummary) {
		if (installState.busy) return
		const instanceLabel = instanceName.trim() || v.id
		installState.begin(v.id, instanceLabel)

		// If a loader is selected, ensure we have versions for THIS MC version
		if (selectedLoader) {
			await ensureLoaderVersions(v)
			if (!selectedLoaderVersion) {
				const label = LOADERS.find((l) => l.id === selectedLoader)?.label ?? selectedLoader
				installState.finish(
					`Не удалось получить версии ${label} для Minecraft ${v.id}`,
				)
				return
			}
		}

		try {
			const instance = await ipc.installVersion(
				v.id,
				instanceLabel,
				selectedLoader ?? undefined,
				selectedLoaderVersion ?? undefined,
			)
			installState.finish()
			oncreated(instance)
		} catch (err) {
			// A user-requested cancel is not an error worth showing in red.
			finishInstall(err)
		}
	}

	/** Opens a file picker for a .mrpack (Modrinth) modpack and imports it. */
	async function importModpack() {
		if (installState.busy) return
		const selected = await open({
			multiple: false,
			filters: [{ name: "Modrinth modpack", extensions: ["mrpack"] }],
		})
		if (!selected || Array.isArray(selected)) return

		const instanceLabel = instanceName.trim() || undefined
		installState.begin(MODPACK_MARKER, instanceLabel ?? "Модпак")
		try {
			const instance = await ipc.importModpack(selected, instanceLabel)
			installState.finish()
			oncreated(instance)
		} catch (err) {
			// A user-requested cancel is not an error worth showing in red.
			finishInstall(err)
		}
	}

	/** Opens a file picker for a Nimbus backup .zip (produced by "Экспорт (.zip)") and imports it as a new instance. */
	async function importBackup() {
		if (installState.busy) return
		const selected = await open({
			multiple: false,
			filters: [{ name: "Резервная копия Nimbus", extensions: ["zip"] }],
		})
		if (!selected || Array.isArray(selected)) return

		const instanceLabel = instanceName.trim() || undefined
		installState.begin(BACKUP_MARKER, instanceLabel ?? "Резервная копия")
		try {
			const instance = await ipc.importInstance(selected, instanceLabel)
			installState.finish()
			oncreated(instance)
		} catch (err) {
			// A user-requested cancel is not an error worth showing in red.
			finishInstall(err)
		}
	}

	// ── Install a modpack straight from Modrinth ────────────────────────
	let modpackQuery = $state("")
	let modpackMcVersion = $state<string | null>(null)
	let modpackSort = $state<ModrinthSort>("downloads")
	let modpackHits = $state<ModrinthHit[]>([])
	let modpackSearching = $state(false)
	let modpackError = $state("")
	let modpackInstallingId = $state<string | null>(null)
	/** Modpack the user opened the Modrinth-style details sheet for. */
	let openPack = $state<ModrinthHit | null>(null)

	async function searchModpacks() {
		const q = modpackQuery.trim()
		modpackSearching = true
		modpackError = ""
		try {
			modpackHits = await ipc.modrinthSearchModpacks(
				q,
				undefined,
				modpackMcVersion ?? undefined,
				modpackSort,
			)
			if (modpackHits.length === 0) modpackError = "Ничего не найдено"
		} catch (err) {
			modpackError = (err as NimbusError).message ?? String(err)
		} finally {
			modpackSearching = false
		}
	}

	/** Downloads the newest compatible version of a Modrinth modpack and installs it as a new instance. */
	async function installModpackFromModrinth(hit: ModrinthHit) {
		if (installState.busy) return
		const instanceLabel = instanceName.trim() || hit.title
		modpackInstallingId = hit.project_id
		installState.begin(MODRINTH_MODPACK_MARKER, instanceLabel)
		try {
			const instance = await ipc.installModpackFromModrinth(hit.project_id, instanceLabel)
			installState.finish()
			oncreated(instance)
		} catch (err) {
			finishInstall(err)
		} finally {
			modpackInstallingId = null
		}
	}

	// Auto-load Modrinth modpacks as soon as this pane is open, sorted like
	// Modrinth itself, and re-run (debounced while typing) on any change.
	$effect(() => {
		const q = modpackQuery
		const mc = modpackMcVersion
		const sort = modpackSort
		void mc
		void sort
		const delay = q.trim() ? 350 : 0
		const timer = setTimeout(() => {
			void searchModpacks()
		}, delay)
		return () => clearTimeout(timer)
	})

	// ── Prism / MultiMC import ───────────────────────────────────────────────
	let prismCandidates = $state<PrismCandidate[]>([])
	let prismScanning = $state(false)
	/** Path of the instance currently being imported, or null. */
	let prismImporting = $state<string | null>(null)
	let prismError = $state("")

	/** Asks for a folder, then lists the instances found inside it. */
	async function pickPrismFolder() {
		prismError = ""
		sound.play("click")
		try {
			const picked = await open({ directory: true, multiple: false })
			if (typeof picked !== "string") return
			prismScanning = true
			prismCandidates = await ipc.scanPrismInstances(picked)
			if (prismCandidates.length === 0) {
				prismError =
					"В этой папке нет сборок Prism/MultiMC. Обычно это %APPDATA%\\PrismLauncher\\instances"
			}
		} catch (err) {
			prismError = (err as NimbusError).message ?? String(err)
		} finally {
			prismScanning = false
		}
	}

	/** Reuses the normal install pipeline, then copies the game folder over. */
	async function importPrism(candidate: PrismCandidate) {
		prismImporting = candidate.path
		prismError = ""
		sound.play("click")
		try {
			const created = await ipc.importPrismInstance(candidate.path, candidate.name)
			prismCandidates = prismCandidates.filter((c) => c.path !== candidate.path)
			oncreated(created)
		} catch (err) {
			prismError = (err as NimbusError).message ?? String(err)
		} finally {
			prismImporting = null
		}
	}

	function fmtBytes(n: number): string {
		if (n >= 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} МБ`
		if (n >= 1024) return `${(n / 1024).toFixed(0)} КБ`
		return `${n} Б`
	}
</script>

<div class="pane">
	{#if installError}
		<div class="flash flash--error anim-fade-up" role="alert">
			<Icon name="alert" size={14} />
			<span class="flash-text">{installError}</span>
			<button class="btn--sm" type="button" onclick={() => installState.clearError()}>Скрыть</button>
		</div>
	{/if}

	<section class="card">
		<div class="card__head">
			<span class="card__title">Загрузчик</span>
			<span class="card__hint">Определяет, какие моды можно установить</span>
		</div>
		<div class="card__body">
			<div class="chips">
				<button
					class="chip"
					class:chip--active={selectedLoader === null}
					type="button"
					disabled={installingId !== null}
					onclick={() => {
						sound.play("click")
						selectedLoader = null
						loaderVersions = []
						selectedLoaderVersion = null
					}}
				>
					Vanilla
				</button>
				{#each LOADERS as l}
					<button
						class="chip"
						class:chip--active={selectedLoader === l.id}
						type="button"
						disabled={installingId !== null}
						onclick={() => {
							sound.play("click")
							selectedLoader = l.id
							loaderForVersion = null
						}}
					>
						{l.label}
					</button>
				{/each}
			</div>

			{#if selectedLoader && loaderPhase === "loading"}
				<div class="note">
					<span class="note-spinner" aria-hidden="true"></span>
					Загрузка версий загрузчика…
				</div>
			{:else if selectedLoader && loaderPhase === "failed"}
				<div class="note note--warn">
					<Icon name="alert" size={13} />
					Не удалось получить версии для этой версии Minecraft
				</div>
			{:else if selectedLoader && selectedLoaderVersion && loaderForVersion}
				<div class="loader-version">
					<label class="field-label" for="loader-version">
						Версия {LOADERS.find((l) => l.id === selectedLoader)?.label}
						<span class="dim">для Minecraft {loaderForVersion}</span>
					</label>
					<select
						id="loader-version"
						class="select"
						bind:value={selectedLoaderVersion}
						disabled={installingId !== null}
					>
						{#each loaderVersions as lv}
							<option value={lv.version}>
								{lv.version}{lv.stable ? " (стабильная)" : ""}
							</option>
						{/each}
					</select>
				</div>
			{:else if selectedLoader && !loaderForVersion}
				<div class="note">Выберите версию Minecraft и нажмите «Установить»</div>
			{/if}
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">Имя сборки</span>
		</div>
		<div class="card__body">
			<input
				id="instance-name"
				class="input"
				type="text"
				placeholder="auto (по версии)"
				aria-label="Имя сборки"
				bind:value={instanceName}
				disabled={installingId !== null}
			/>
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">
				Версия Minecraft
				{#if phase.kind === "ready"}
					<span class="count tnum">{filtered.length} из {versions.length}</span>
				{/if}
			</span>
			<div class="head-tools">
				<label class="toggle">
					<input
						type="checkbox"
						class="toggle__input"
						bind:checked={includeSnapshots}
						disabled={installingId !== null}
						onchange={() => sound.play("toggle")}
					/>
					<span class="toggle__track"></span>
					<span class="toggle-text">Снапшоты</span>
				</label>
				<div class="mini-search">
					<span class="mini-search-icon" aria-hidden="true"><Icon name="search" size={12} /></span>
					<input
						class="mini-search-input"
						type="text"
						placeholder="1.20.1, 1 21, релиз"
						aria-label="Поиск версии"
						bind:value={search}
						disabled={installingId !== null}
					/>
					{#if search}
						<button
							class="mini-search-clear"
							type="button"
							aria-label="Очистить поиск"
							onclick={() => (search = "")}
						>
							<Icon name="close" size={11} strokeWidth={2.2} />
						</button>
					{/if}
				</div>
			</div>
		</div>

		{#if phase.kind === "loading"}
			<div class="rows">
				{#each { length: 8 } as _}
					<div class="vrow vrow--skeleton">
						<Skeleton width="96px" height="13px" />
						<Skeleton width="64px" height="11px" />
					</div>
				{/each}
			</div>
		{:else if phase.kind === "failed"}
			<EmptyState
				icon="alert"
				tone="danger"
				title="Не удалось получить список версий"
				body={phase.message}
				actionLabel="Повторить"
				onaction={() => void load()}
			/>
		{:else if filtered.length === 0}
			{#if snapshotHint}
				<EmptyState
					icon="cube"
					title="Среди релизов ничего не найдено"
					body="Возможно, это снапшот или старая тестовая версия."
					actionLabel="Включить снапшоты"
					onaction={() => (includeSnapshots = true)}
				/>
			{:else}
				<EmptyState
					icon="cube"
					title="Ничего не найдено"
					body="Измените запрос или включите снапшоты."
					actionLabel={search ? "Очистить поиск" : undefined}
					onaction={() => (search = "")}
				/>
			{/if}
		{:else}
			<div class="rows">
				{#each filtered as v (v.id)}
					<div class="vrow">
						<div class="vrow-main">
							<span class="vid">{v.id}</span>
							<span class="vmeta">
								{v.type === "release" ? "релиз" : v.type}
								· {new Date(v.releaseTime).toLocaleDateString("ru-RU")}
							</span>
						</div>
						{#if installingId === v.id}
							<div class="installing">
								<div class="progress">
									<div class="progress__bar" style="width: {progressPct}%"></div>
								</div>
								<div class="installing-row">
									<span class="stage tnum">
										{STAGE_LABELS[progress?.stage ?? ""] ?? "Подготовка"}
										{#if progress && progress.total > 0}
											· {progress.done}/{progress.total}
										{:else if progress && progress.bytesDone > 0}
											· {fmtBytes(progress.bytesDone)}
										{/if}
										{#if rate}
											· {rate}
										{/if}
									</span>
									<button
										class="btn--sm"
										type="button"
										disabled={installState.cancelling}
										onclick={() => void installState.cancel()}
									>
										{installState.cancelling ? "Отмена…" : "Отменить"}
									</button>
								</div>
							</div>
						{:else}
							<button
								class="btn--sm vrow-cta"
								type="button"
								disabled={installingId !== null}
								onclick={() => {
									sound.play("click")
									void install(v)
								}}
							>
								<Icon name="download" size={13} />
								Установить
							</button>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">Импорт</span>
			<span class="card__hint">Готовый модпак или резервная копия</span>
		</div>
		<div class="card__body import">
			<div class="import-row">
				<button
					class="btn"
					type="button"
					disabled={installingId !== null}
					onclick={() => {
						sound.play("click")
						void importModpack()
					}}
				>
					<Icon name="package" size={15} />
					Модпак (.mrpack)
				</button>
				<button
					class="btn"
					type="button"
					disabled={installingId !== null}
					onclick={() => {
						sound.play("click")
						void importBackup()
					}}
				>
					<Icon name="upload" size={15} />
					Резервная копия (.zip)
				</button>
				<button
					class="btn"
					type="button"
					disabled={installingId !== null || prismScanning}
					onclick={() => void pickPrismFolder()}
				>
					<Icon name="folderPlus" size={15} />
					{prismScanning ? "Поиск сборок…" : "Из Prism / MultiMC"}
				</button>
			</div>

			{#if prismError}
				<p class="prism-error" role="alert">{prismError}</p>
			{/if}

			{#if prismCandidates.length > 0}
				<div class="prism anim-fade-up">
					<div class="prism-head">
						<span class="prism-title">
							Найдено сборок: <span class="tnum">{prismCandidates.length}</span>
						</span>
						<button class="btn--sm" type="button" onclick={() => (prismCandidates = [])}>
							Скрыть
						</button>
					</div>
					{#each prismCandidates as candidate (candidate.path)}
						<div class="vrow">
							<div class="vrow-main">
								<span class="vid">{candidate.name}</span>
								<span class="vmeta tnum">
									{candidate.loader ?? "Vanilla"} · {candidate.minecraftVersion}
									{#if candidate.modsCount > 0}
										· модов: {candidate.modsCount}
									{/if}
									{#if candidate.sizeBytes > 0}
										· {fmtBytes(candidate.sizeBytes)}
									{/if}
								</span>
							</div>
							<button
								class="btn--sm"
								type="button"
								disabled={prismImporting !== null || installingId !== null}
								onclick={() => void importPrism(candidate)}
							>
								{prismImporting === candidate.path ? "Импорт…" : "Импортировать"}
							</button>
						</div>
					{/each}
					<p class="prism-hint">
						Игра и загрузчик будут скачаны заново в общий кэш, а папка с модами,
						мирами и настройками скопируется как есть. Исходная сборка не изменится.
					</p>
				</div>
			{/if}

			{#if installingId === MODPACK_MARKER || installingId === BACKUP_MARKER || installingId === MODRINTH_MODPACK_MARKER}
				<div class="installing anim-fade-up">
					<div class="progress">
						<div class="progress__bar" style="width: {progressPct}%"></div>
					</div>
					<div class="installing-row">
						<span class="stage tnum">
							{STAGE_LABELS[progress?.stage ?? ""] ?? "Подготовка"}
							{#if progress && progress.total > 0}
								· {progress.done}/{progress.total}
							{:else if progress && progress.bytesDone > 0}
								· {fmtBytes(progress.bytesDone)}
							{/if}
							{#if rate}
								· {rate}
							{/if}
						</span>
						<button
							class="btn--sm"
							type="button"
							disabled={installState.cancelling}
							onclick={() => void installState.cancel()}
						>
							{installState.cancelling ? "Отмена…" : "Отменить"}
						</button>
					</div>
				</div>
			{/if}
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">Модпак с Modrinth</span>
			<span class="card__hint">Установить готовую сборку по названию</span>
		</div>
		<div class="card__body import">
			<div class="import-row">
				<div class="mini-search">
					<span class="mini-search-icon" aria-hidden="true"><Icon name="search" size={12} /></span>
					<input
						class="mini-search-input"
						type="text"
						placeholder="Название модпака"
						aria-label="Поиск модпака на Modrinth"
						bind:value={modpackQuery}
						disabled={installingId !== null}
						onkeydown={(e) => {
							if (e.key === "Enter") void searchModpacks()
						}}
					/>
				</div>
				<select
					class="mini-select"
					bind:value={modpackMcVersion}
					disabled={installingId !== null}
					aria-label="Версия Minecraft"
				>
					<option value={null}>Любая версия</option>
					{#each releaseVersions as v (v.id)}
						<option value={v.id}>{v.id}</option>
					{/each}
				</select>
				<select
					class="mini-select"
					bind:value={modpackSort}
					disabled={installingId !== null}
					aria-label="Сортировка"
				>
					<option value="downloads">По загрузкам</option>
					<option value="follows">По подпискам</option>
					<option value="newest">Сначала новые</option>
					<option value="updated">По обновлению</option>
					<option value="relevance">По релевантности</option>
				</select>
				<button
					class="btn"
					type="button"
					disabled={modpackSearching || installingId !== null}
					onclick={() => void searchModpacks()}
				>
					<Icon name="search" size={15} />
					{modpackSearching ? "Поиск…" : "Найти"}
				</button>
			</div>

			{#if modpackError}
				<p class="prism-error" role="alert">{modpackError}</p>
			{/if}

			{#if modpackHits.length > 0}
				<div class="rows">
					{#each modpackHits as hit (hit.project_id)}
						<div class="vrow">
							<button
								class="vrow-open"
								type="button"
								title="Открыть описание"
								onclick={() => {
									sound.play("click")
									openPack = hit
								}}
							>
								{#if hit.icon_url}
									<img class="vrow-icon" src={hit.icon_url} alt="" width="32" height="32" />
								{:else}
									<div class="vrow-icon vrow-icon--blank" aria-hidden="true">
										<Icon name="package" size={14} />
									</div>
								{/if}
								<div class="vrow-main">
									<span class="vid">{hit.title}</span>
									<span class="vmeta">{hit.author ?? ""}</span>
								</div>
							</button>
							<button
								class="btn--sm vrow-cta"
								type="button"
								disabled={installingId !== null}
								onclick={() => void installModpackFromModrinth(hit)}
							>
								{modpackInstallingId === hit.project_id ? "Установка…" : "Установить"}
							</button>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	</section>
</div>

{#if openPack}
	<ModDetails
		projectId={openPack.project_id}
		title={openPack.title}
		installing={modpackInstallingId === openPack.project_id}
		installLabel="Установить сборку"
		versionPicker={false}
		oninstall={() => {
			const pack = openPack
			if (!pack) return
			openPack = null
			void installModpackFromModrinth(pack)
		}}
		onclose={() => (openPack = null)}
	/>
{/if}

<style>
	.pane {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
		width: 100%;
		max-width: 880px;
		margin: 0 auto;
		padding: var(--sp-6);
	}

	.flash {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-3) var(--sp-4);
		border-radius: var(--r-md);
		font-size: var(--fs-small);
	}
	.flash--error {
		color: var(--danger);
		background: var(--danger-soft);
		box-shadow: inset 0 0 0 1px rgba(242, 85, 90, 0.3);
	}
	.flash-text {
		flex: 1;
		min-width: 0;
	}

	.card {
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

	.card__hint {
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}

	.card__body {
		padding: var(--sp-5);
	}

	.count {
		font-size: var(--fs-micro);
		font-weight: var(--fw-regular);
		color: var(--text-tertiary);
	}

	.head-tools {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
	}

	.toggle-text {
		font-size: var(--fs-small);
		color: var(--text-secondary);
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--sp-2);
	}

	.note {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		margin-top: var(--sp-4);
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}
	.note--warn {
		color: var(--warn);
	}

	.note-spinner {
		width: 12px;
		height: 12px;
		border-radius: var(--r-full);
		border: 2px solid var(--border-strong);
		border-top-color: var(--accent);
		animation: spinSlow 700ms linear infinite;
	}

	.loader-version {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		margin-top: var(--sp-4);
	}

	.field-label {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-secondary);
	}

	.dim {
		font-weight: var(--fw-regular);
		color: var(--text-tertiary);
	}

	.select {
		width: 100%;
		max-width: 320px;
		min-height: 34px;
		padding: 0 var(--sp-3);
		border: 0;
		border-radius: var(--r-md);
		background: var(--bg-inset);
		color: var(--text-primary);
		font-size: var(--fs-body);
		box-shadow: var(--edge-ring);
		transition: box-shadow var(--dur-fast) var(--ease-out);
	}
	.select:hover {
		box-shadow: inset 0 0 0 1px var(--border-strong);
	}
	.select:focus {
		outline: none;
		box-shadow:
			inset 0 0 0 1px var(--accent-border), 0 0 0 3px var(--accent-soft);
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
		width: 190px;
		height: 30px;
		padding: 0 24px 0 26px;
		border: 0;
		border-radius: var(--r-sm);
		background: var(--bg-inset);
		color: var(--text-primary);
		font-size: var(--fs-small);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
		user-select: text;
		-webkit-user-select: text;
		transition: box-shadow var(--dur-fast) var(--ease-out);
	}
	.mini-search-input::placeholder {
		color: var(--text-tertiary);
	}
	.mini-search-input:focus {
		outline: none;
		box-shadow:
			inset 0 0 0 1px var(--accent-border), 0 0 0 3px var(--accent-soft);
	}

	.mini-search-clear {
		position: absolute;
		right: 5px;
		display: grid;
		place-items: center;
		width: 18px;
		height: 18px;
		border-radius: var(--r-full);
		color: var(--text-tertiary);
	}
	.mini-search-clear:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.mini-select {
		height: 30px;
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

	/* ── Version rows ──────────────────────────────────────────── */

	.rows {
		display: flex;
		flex-direction: column;
		padding: var(--sp-2);
		max-height: 420px;
		overflow-y: auto;
	}

	.vrow {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
		min-height: 44px;
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-md);
		transition: background var(--dur-fast) var(--ease-out);
	}
	.vrow:hover {
		background: var(--bg-hover);
	}
	.vrow--skeleton {
		flex-direction: column;
		align-items: flex-start;
		gap: var(--sp-2);
		pointer-events: none;
	}

	/* Clickable part of a modpack row: opens the Modrinth-style details sheet. */
	.vrow-open {
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

	.vrow-icon {
		flex: none;
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		border-radius: var(--r-sm);
		object-fit: cover;
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
		color: var(--text-tertiary);
	}

	.vrow-main {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.vid {
		font-size: var(--fs-body);
		font-weight: var(--fw-medium);
		font-variant-numeric: tabular-nums;
		color: var(--text-primary);
	}

	.vmeta {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.vrow-cta {
		opacity: 0.6;
		transition: opacity var(--dur-fast) var(--ease-out);
	}
	.vrow:hover .vrow-cta,
	.vrow:focus-within .vrow-cta {
		opacity: 1;
	}

	/* ── Install progress ────────────────────────────── */

	.installing {
		flex: 1;
		max-width: 460px;
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
	}

	.progress {
		height: 4px;
		border-radius: var(--r-full);
		background: var(--bg-active);
		overflow: hidden;
	}

	.progress__bar {
		height: 100%;
		border-radius: var(--r-full);
		background: var(--accent);
		box-shadow: 0 0 12px -2px var(--accent-glow);
		transition: width var(--dur-base) var(--ease-out);
	}

	.installing-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-3);
	}

	.stage {
		min-width: 0;
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* ── Import ────────────────────────────────────── */

	.import {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	.import-row {
		display: flex;
		flex-wrap: wrap;
		gap: var(--sp-2);
	}

	.prism {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: var(--sp-2);
		border-radius: var(--r-md);
		background: var(--bg-surface);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
	}

	.prism-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--sp-1) var(--sp-2) var(--sp-2);
	}

	.prism-title {
		font-size: var(--fs-small);
		font-weight: var(--fw-semibold);
		color: var(--text-secondary);
	}

	.prism-hint {
		padding: var(--sp-2);
		font-size: var(--fs-micro);
		line-height: 1.55;
		color: var(--text-tertiary);
	}

	.prism-error {
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-sm);
		font-size: var(--fs-small);
		color: var(--danger);
		background: var(--danger-soft);
	}

	@media (prefers-reduced-motion: reduce) {
		.note-spinner {
			animation: none;
		}
	}
</style>
