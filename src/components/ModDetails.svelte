<script lang="ts">
	// Modrinth-style project page shown as a modal sheet: icon, stats, gallery,
	// the full Markdown description and a version picker with an install action.
	// Used by both catalogues (mods inside an instance, modpacks on creation).
	import Icon from "./Icon.svelte"
	import {
		ipc,
		type ModrinthProject,
		type ModrinthVersion,
		type NimbusError,
	} from "$lib/ipc"
	import { renderMarkdown } from "$lib/markdown"
	import { locale, t } from "$lib/i18n.svelte"
	import { sound } from "$lib/sound.svelte"

	let {
		projectId,
		title,
		instanceId = null,
		installing = false,
		installLabel = t("Установить"),
		versionPicker = true,
		oninstall,
		onclose,
	}: {
		projectId: string
		title: string
		/** When set, versions are narrowed to that instance's loader and MC version. */
		instanceId?: string | null
		installing?: boolean
		installLabel?: string
		/** Modpack installs always take the newest version, so the picker is hidden there. */
		versionPicker?: boolean
		oninstall: (versionId: string | null) => void
		onclose: () => void
	} = $props()

	let project = $state<ModrinthProject | null>(null)
	let versions = $state<ModrinthVersion[]>([])
	let loading = $state(true)
	let loadError = $state<string | null>(null)
	/** `null` means "newest compatible version", which the backend picks itself. */
	let selectedVersion = $state<string | null>(null)
	let shotIndex = $state(0)

	const body = $derived(project?.body ? renderMarkdown(project.body) : "")

	/**
	 * Description links must reach the system browser: letting the WebView
	 * navigate would replace the launcher with the website, with no way back.
	 */
	function openBodyLink(event: MouseEvent) {
		const link = (event.target as HTMLElement | null)?.closest("a.md-link")
		if (!link) return
		event.preventDefault()
		const url = link.getAttribute("href")
		if (url) void ipc.openUrl(url).catch(() => {})
	}
	const gallery = $derived(project?.gallery ?? [])
	const shot = $derived(gallery[shotIndex] ?? null)

	function msgOf(err: unknown): string {
		return (err as NimbusError).message ?? String(err)
	}

	function fmtDownloads(n: number): string {
		if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
		if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
		return String(n)
	}

	function fmtDate(iso: string | null): string {
		if (!iso) return "—"
		const date = new Date(iso)
		return Number.isNaN(date.getTime()) ? "—" : date.toLocaleDateString(locale())
	}

	async function load(id: string, forInstance: string | null) {
		loading = true
		loadError = null
		try {
			const [loaded, list] = await Promise.all([
				ipc.modrinthProject(id),
				forInstance
					? ipc.modrinthVersions(forInstance, id)
					: ipc.modrinthProjectVersions(id),
			])
			project = loaded
			versions = list
		} catch (err) {
			loadError = msgOf(err)
		} finally {
			loading = false
		}
	}

	$effect(() => {
		shotIndex = 0
		selectedVersion = null
		void load(projectId, instanceId)
	})

	function close() {
		sound.play("click")
		onclose()
	}
</script>

<svelte:window
	onkeydown={(e) => {
		if (e.key === "Escape") onclose()
	}}
/>

<button class="scrim" type="button" aria-label={t("Закрыть описание")} onclick={close}
></button>

<div class="sheet" role="dialog" aria-modal="true" aria-label={title}>
	<header class="sheet-head">
		{#if project?.icon_url}
			<img class="sheet-icon" src={project.icon_url} alt="" width="56" height="56" />
		{:else}
			<div class="sheet-icon sheet-icon--blank" aria-hidden="true">
				<Icon name="package" size={22} />
			</div>
		{/if}
		<div class="sheet-head-main">
			<span class="sheet-title">{project?.title ?? title}</span>
			<span class="sheet-sub">{project?.description ?? ""}</span>
		</div>
		<button class="btn--sm" type="button" aria-label={t("Закрыть")} onclick={close}>
			<Icon name="close" size={14} />
		</button>
	</header>

	<div class="sheet-body">
		{#if loading}
			<div class="void"><span class="void-title">{t("Загрузка описания…")}</span></div>
		{:else if loadError}
			<div class="inline-error" role="alert">{loadError}</div>
		{:else if project}
			<div class="stats">
				<span class="stat tnum">{fmtDownloads(project.downloads)} загрузок</span>
				<span class="stat tnum">{fmtDownloads(project.followers)} подписчиков</span>
				<span class="stat">Обновлён {fmtDate(project.updated)}</span>
				{#if project.license?.id}
					<span class="stat">Лицензия {project.license.id}</span>
				{/if}
			</div>

			{#if project.categories.length > 0}
				<div class="chips">
					{#each project.categories as category (category)}
						<span class="chip">{category}</span>
					{/each}
				</div>
			{/if}

			{#if shot}
				<figure class="shot">
					<img src={shot.url} alt={shot.title ?? ""} loading="lazy" />
					{#if gallery.length > 1}
						<figcaption class="shot-nav">
							<button
								class="btn--sm"
								type="button"
								aria-label={t("Предыдущий скриншот")}
								onclick={() =>
									(shotIndex = (shotIndex - 1 + gallery.length) % gallery.length)}
							>
								←
							</button>
							<span class="tnum">{shotIndex + 1} / {gallery.length}</span>
							<button
								class="btn--sm"
								type="button"
								aria-label={t("Следующий скриншот")}
								onclick={() => (shotIndex = (shotIndex + 1) % gallery.length)}
							>
								→
							</button>
						</figcaption>
					{/if}
				</figure>
			{/if}

			{#if body}
				<!-- Sanitised in renderMarkdown: the source is escaped before any tag is added. -->
				<!-- Delegated so every link in the rendered body is covered by one handler. -->
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="md" onclick={openBodyLink}>{@html body}</div>
			{:else}
				<p class="hint">{t("Автор не добавил подробное описание.")}</p>
			{/if}
		{/if}
	</div>

	<footer class="sheet-foot">
		{#if versionPicker}
		<select
			class="mini-select"
			bind:value={selectedVersion}
			disabled={loading || installing || versions.length === 0}
			aria-label={t("Версия")}
		>
			<option value={null}>{t("Последняя совместимая")}</option>
			{#each versions as v (v.id)}
				<option value={v.id}>{v.version_number} · {v.game_versions.join(", ")}</option>
			{/each}
		</select>
		{/if}
		<button
			class="btn--sm btn--on"
			type="button"
			disabled={installing}
			onclick={() => oninstall(selectedVersion)}
		>
			<Icon name="download" size={13} />
			{installing ? t("Установка…") : installLabel}
		</button>
	</footer>
</div>

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: var(--z-modal);
		border: 0;
		padding: 0;
		background: var(--bg-scrim);
		cursor: default;
	}

	/*
	 * Centred with inset + margin instead of a translate, so no global animation
	 * class (which animates `transform`) can knock the dialog off screen.
	 */
	.sheet {
		position: fixed;
		inset: var(--sp-6);
		margin: auto;
		z-index: calc(var(--z-modal) + 1);
		display: flex;
		flex-direction: column;
		width: min(760px, calc(100vw - var(--sp-8)));
		height: max-content;
		max-height: calc(100vh - var(--sp-12));
		animation: sheetIn var(--dur-base) var(--ease-out) both;
		border-radius: var(--r-lg);
		background: var(--bg-surface);
		box-shadow:
			inset 0 0 0 1px var(--border),
			0 24px 64px rgba(0, 0, 0, 0.55);
		overflow: hidden;
	}

	@keyframes sheetIn {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	.sheet-head {
		display: flex;
		align-items: flex-start;
		gap: var(--sp-3);
		padding: var(--sp-4);
		border-bottom: 1px solid var(--border-subtle);
	}
	.sheet-icon {
		flex: none;
		width: 56px;
		height: 56px;
		border-radius: var(--r-md);
		object-fit: cover;
		background: var(--bg-raised);
	}
	.sheet-icon--blank {
		display: grid;
		place-items: center;
		color: var(--text-tertiary);
	}
	.sheet-head-main {
		display: flex;
		flex-direction: column;
		gap: var(--sp-1);
		flex: 1;
		min-width: 0;
	}
	.sheet-title {
		font-family: var(--font-display);
		font-size: var(--fs-title);
		font-weight: var(--fw-semibold);
		color: var(--text-primary);
	}
	.sheet-sub {
		font-size: var(--fs-small);
		color: var(--text-secondary);
	}

	.sheet-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: var(--sp-4);
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
	}

	.stats {
		display: flex;
		flex-wrap: wrap;
		gap: var(--sp-3);
		font-size: var(--fs-small);
		color: var(--text-secondary);
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--sp-2);
	}
	.chip {
		padding: 2px var(--sp-2);
		border-radius: var(--r-full);
		background: var(--bg-raised);
		font-size: var(--fs-micro);
		color: var(--text-secondary);
	}

	.shot {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
	}
	.shot img {
		width: 100%;
		border-radius: var(--r-md);
		background: var(--bg-inset);
	}
	.shot-nav {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--sp-3);
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}

	.md {
		font-size: var(--fs-body);
		line-height: var(--lh-body);
		color: var(--text-secondary);
		word-break: break-word;
	}
	.md :global(h2),
	.md :global(h3),
	.md :global(h4),
	.md :global(h5),
	.md :global(h6) {
		margin: var(--sp-4) 0 var(--sp-2);
		font-size: var(--fs-title);
		color: var(--text-primary);
	}
	.md :global(p),
	.md :global(ul),
	.md :global(ol) {
		margin: 0 0 var(--sp-3);
	}
	.md :global(ul),
	.md :global(ol) {
		padding-left: var(--sp-5);
	}
	.md :global(img) {
		max-width: 100%;
		border-radius: var(--r-sm);
	}
	.md :global(code) {
		font-family: var(--font-mono);
		font-size: var(--fs-small);
		color: var(--text-primary);
	}
	.md :global(pre) {
		padding: var(--sp-3);
		border-radius: var(--r-sm);
		background: var(--bg-inset);
		overflow-x: auto;
	}
	.md :global(blockquote) {
		margin: 0 0 var(--sp-3);
		padding-left: var(--sp-3);
		border-left: 2px solid var(--border-strong);
	}
	.md :global(.md-link) {
		color: var(--text-primary);
		text-decoration: underline;
		text-underline-offset: 2px;
		cursor: pointer;
	}

	.sheet-foot {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: var(--sp-2);
		padding: var(--sp-3) var(--sp-4);
		border-top: 1px solid var(--border-subtle);
		background: var(--bg-raised);
	}
</style>
