<script lang="ts">
	import { untrack } from "svelte"
	import Icon from "./Icon.svelte"
	import ModDetails from "./ModDetails.svelte"
	import Pagination from "./Pagination.svelte"
	import { ipc, type ContentKind, type ModrinthHit, type ModrinthSort, type NimbusError } from "$lib/ipc"
	import { sound } from "$lib/sound.svelte"
	import { toasts } from "$lib/toast.svelte"
	import { t, tf } from "$lib/i18n.svelte"

	let {
		instanceId,
		kind,
	}: {
		instanceId: string
		/** Resource packs and shaders differ only in target folder and wording. */
		kind: ContentKind
	} = $props()

	const PAGE_SIZE = 20

	let query = $state("")
	let sort = $state<ModrinthSort>("downloads")
	let page = $state(1)
	let hits = $state<ModrinthHit[]>([])
	let totalHits = $state(0)
	let searching = $state(false)
	let searched = $state(false)
	let error = $state<string | null>(null)
	let searchSeq = 0

	let installed = $state<string[]>([])
	let installedLoading = $state(false)
	/** Project ids currently downloading, so each card shows its own spinner. */
	let installing = $state<Set<string>>(new Set())
	let removing = $state<string | null>(null)
	let openHit = $state<ModrinthHit | null>(null)
	let failedIcons = $state<Set<string>>(new Set())

	const totalPages = $derived(Math.min(100, Math.max(1, Math.ceil(totalHits / PAGE_SIZE))))
	const labels = $derived(
		kind === "resourcepack"
			? {
					title: t("Ресурспаки"),
					hint: t(
						"Скачиваются сразу в сборку — останется включить их в настройках игры.",
					),
					search: t("Найти ресурспак"),
					empty: t("Ресурспаки пока не установлены."),
					install: t("Установить ресурспак"),
				}
			: {
					title: t("Шейдеры"),
					hint: t(
						"Скачиваются сразу в сборку. Для работы нужен Iris или OptiFine в модах.",
					),
					search: t("Найти шейдер"),
					empty: t("Шейдеры пока не установлены."),
					install: t("Установить шейдер"),
				},
	)

	function fmtDownloads(n: number): string {
		if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M"
		if (n >= 1_000) return Math.round(n / 1_000) + "K"
		return String(n)
	}

	function msgOf(err: unknown): string {
		return (err as NimbusError).message ?? String(err)
	}

	function failIcon(url: string) {
		failedIcons = new Set([...failedIcons, url])
	}

	async function loadInstalled() {
		installedLoading = true
		try {
			installed = await ipc.listInstanceContent(instanceId, kind)
		} catch {
			// An unreadable folder is not worth an error banner: it usually just does not exist yet.
			installed = []
		} finally {
			installedLoading = false
		}
	}

	async function search(nextPage = 1) {
		const seq = ++searchSeq
		const next = Math.max(1, nextPage)
		page = next
		searching = true
		error = null
		try {
			const result = await ipc.modrinthSearchContent(
				instanceId,
				kind,
				query.trim(),
				PAGE_SIZE,
				(next - 1) * PAGE_SIZE,
				sort,
			)
			if (seq !== searchSeq) return
			hits = result.hits
			totalHits = result.totalHits
			searched = true
		} catch (err) {
			if (seq !== searchSeq) return
			error = msgOf(err)
			sound.play("error")
		} finally {
			if (seq === searchSeq) searching = false
		}
	}

	async function install(hit: ModrinthHit, versionId: string | null = null) {
		if (installing.has(hit.project_id)) return
		installing = new Set([...installing, hit.project_id])
		error = null
		sound.play("click")
		try {
			const fileName = await ipc.modrinthInstallContent(
				instanceId,
				kind,
				hit.project_id,
				versionId ?? undefined,
			)
			await loadInstalled()
			sound.play("success")
			toasts.success(tf("Установлено: {0}", fileName))
			openHit = null
		} catch (err) {
			error = msgOf(err)
			sound.play("error")
		} finally {
			const next = new Set(installing)
			next.delete(hit.project_id)
			installing = next
		}
	}

	async function remove(fileName: string) {
		removing = fileName
		sound.play("click")
		try {
			await ipc.removeInstanceContent(instanceId, kind, fileName)
			await loadInstalled()
			toasts.info(tf("Удалено: {0}", fileName))
		} catch (err) {
			error = msgOf(err)
		} finally {
			removing = null
		}
	}

	// Reload whenever the instance or the content kind changes. The body runs
	// untracked: search() reads and writes the same state this effect touches.
	$effect(() => {
		const id = instanceId
		const k = kind
		untrack(() => {
			void id
			void k
			query = ""
			sort = "downloads"
			page = 1
			hits = []
			totalHits = 0
			searched = false
			error = null
			openHit = null
			failedIcons = new Set()
			void loadInstalled()
			void search(1)
		})
	})
</script>

<div class="wrap">
	<section class="block">
		<div class="block__head">
			<div class="block__text">
				<span class="block__title">{labels.title}</span>
				<span class="block__hint">{labels.hint}</span>
			</div>
		</div>

		<div class="searchbar">
			<input
				class="input"
				type="text"
				placeholder={labels.search}
				aria-label={labels.search}
				bind:value={query}
				onkeydown={(e) => {
					if (e.key === "Enter") void search(1)
				}}
			/>
			<select
				class="mini-select sort"
				bind:value={sort}
				aria-label={t("Сортировка")}
				onchange={() => void search(1)}
			>
				<option value="downloads">{t("По загрузкам")}</option>
				<option value="follows">{t("По подпискам")}</option>
				<option value="newest">{t("Сначала новые")}</option>
				<option value="updated">{t("По обновлению")}</option>
				<option value="relevance">{t("По релевантности")}</option>
			</select>
			<button class="btn--sm btn--on" type="button" disabled={searching} onclick={() => void search(1)}>
				{searching ? t("Поиск…") : t("Искать")}
			</button>
		</div>

		{#if error}
			<div class="alert alert--danger" role="alert">
				<span class="alert-icon" aria-hidden="true"><Icon name="alert" size={14} /></span>
				<span class="alert-text">{error}</span>
			</div>
		{/if}

		{#if searching && hits.length === 0}
			<div class="catalog-state" aria-live="polite">
				<span class="state-spinner" aria-hidden="true"></span>
				<span>{t("Загрузка каталога…")}</span>
			</div>
		{:else if hits.length === 0 && searched}
			<p class="muted">{t("Ничего не найдено. Попробуйте другой запрос.")}</p>
		{:else if hits.length > 0}
			<ul class="cards">
				{#each hits as hit (hit.project_id)}
					<li class="card content-card">
						<button
							class="card-open"
							type="button"
							aria-label={tf("Открыть описание: {0}", hit.title)}
							onclick={() => {
								sound.play("click")
								openHit = hit
							}}
						>
							{#if hit.icon_url && !failedIcons.has(hit.icon_url)}
								<img
									class="card__icon"
									src={hit.icon_url}
									alt=""
									width="48"
									height="48"
									loading="lazy"
									onerror={() => failIcon(hit.icon_url!)}
								/>
							{:else}
								<span class="card__icon card__icon--blank" aria-hidden="true">
									<Icon name="package" size={18} />
								</span>
							{/if}
							<span class="card__body">
								<span class="card__title">{hit.title}</span>
								<span class="card__desc">{hit.description}</span>
								<span class="card__meta">
									<span class="tnum">{fmtDownloads(hit.downloads)}</span>
									{t("загрузок")}
									{#if hit.author}<span class="dot">·</span>{hit.author}{/if}
								</span>
							</span>
						</button>
						<button
							class="btn--sm"
							type="button"
							disabled={installing.has(hit.project_id)}
							onclick={() => void install(hit)}
						>
							<Icon name="download" size={13} />
							{installing.has(hit.project_id) ? t("Скачивание…") : t("Установить")}
						</button>
					</li>
				{/each}
			</ul>
			<Pagination page={page} totalPages={totalPages} disabled={searching} onchange={(p) => void search(p)} />
		{/if}
	</section>

	<section class="block">
		<div class="block__head">
			<div class="block__text">
				<span class="block__title">{t("Установлено в сборке")}</span>
				<span class="block__hint">
					{installedLoading ? t("Проверка папки…") : tf("Файлов: {0}", String(installed.length))}
				</span>
			</div>
		</div>

		{#if installedLoading}
			<p class="muted">{t("Проверка папки…")}</p>
		{:else if installed.length === 0}
			<p class="muted">{labels.empty}</p>
		{:else}
			<ul class="files">
				{#each installed as name (name)}
					<li class="file">
						<span class="file__name">{name}</span>
						<button
							class="file__kill"
							type="button"
							title={t("Удалить")}
							aria-label={t("Удалить")}
							disabled={removing === name}
							onclick={() => void remove(name)}
						>
							<Icon name="trash" size={13} />
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
</div>

{#if openHit}
	<ModDetails
		projectId={openHit.project_id}
		title={openHit.title}
		instanceId={instanceId}
		contentKind={kind}
		installing={installing.has(openHit.project_id)}
		installLabel={labels.install}
		oninstall={(versionId) => void install(openHit!, versionId)}
		onclose={() => (openHit = null)}
	/>
{/if}

<style>
	.wrap {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	.block {
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-card);
	}

	.block__head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
	}

	.block__text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.block__title {
		font-size: var(--fs-body);
		font-weight: var(--fw-semibold);
		color: var(--text-primary);
	}

	.block__hint {
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}

	.searchbar {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
	}
	.searchbar .input {
		flex: 1;
		min-width: 0;
	}
	.searchbar .sort {
		flex: none;
		width: 148px;
	}

	.muted {
		margin: 0;
		font-size: var(--fs-small);
		line-height: var(--lh-body);
		color: var(--text-tertiary);
	}

	.catalog-state {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		min-height: 44px;
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}
	.state-spinner {
		width: 14px;
		height: 14px;
		border: 2px solid var(--border-strong);
		border-top-color: var(--accent);
		border-radius: var(--r-full);
		animation: spinSlow 700ms linear infinite;
	}

	.cards {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.content-card {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		min-width: 0;
		padding: var(--sp-3);
		border-radius: var(--r-md);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
	}

	.card-open {
		display: flex;
		align-items: center;
		flex: 1;
		min-width: 0;
		gap: var(--sp-3);
		padding: 0;
		border: 0;
		background: transparent;
		text-align: left;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}

	.card__icon {
		flex: none;
		width: 48px;
		height: 48px;
		border-radius: var(--r-md);
		object-fit: cover;
		background: var(--bg-raised);
		box-shadow: var(--edge-ring);
	}
	.card__icon--blank {
		display: grid;
		place-items: center;
		color: var(--text-tertiary);
	}

	.card__body {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.card__title {
		font-size: var(--fs-body);
		font-weight: var(--fw-medium);
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.card__desc {
		display: -webkit-box;
		overflow: hidden;
		color: var(--text-secondary);
		font-size: var(--fs-small);
		text-overflow: ellipsis;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 2;
		line-clamp: 2;
	}

	.card__meta {
		display: flex;
		gap: 5px;
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}
	.dot {
		opacity: 0.6;
	}

	.files {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.file {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-sm);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
	}

	.file__name {
		flex: 1;
		min-width: 0;
		font-family: var(--font-mono);
		font-size: var(--fs-small);
		color: var(--text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.file__kill {
		flex: none;
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: var(--r-sm);
		color: var(--text-tertiary);
		transition: color var(--dur-fast) var(--ease-out);
	}
	.file__kill:hover {
		color: var(--danger);
		background: var(--danger-soft);
	}

	@media (max-width: 640px) {
		.searchbar {
			flex-wrap: wrap;
		}
		.searchbar .input {
			flex-basis: 100%;
		}
		.searchbar .sort {
			flex: 1;
			width: auto;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.state-spinner {
			animation: none;
		}
	}
</style>
