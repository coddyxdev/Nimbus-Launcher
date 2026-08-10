<script lang="ts">
	import { onMount } from "svelte"
	import { ipc, type NewsItem, type NimbusError } from "$lib/ipc"
	import { i18n, locale, t } from "$lib/i18n.svelte"
	import { sound } from "$lib/sound.svelte"
	import { toasts } from "$lib/toast.svelte"
	import { CHANGELOG } from "$lib/changelog"
	import EmptyState from "./EmptyState.svelte"
	import Icon from "./Icon.svelte"

	let { onwhatsnew }: { onwhatsnew?: () => void } = $props()

	let items = $state<NewsItem[]>([])
	let loading = $state(true)
	let error = $state<string | null>(null)

	const ru = $derived(i18n.current === "ru")
	/** Release notes for the running build, offered next to the feed. */
	const latestRelease = $derived(CHANGELOG[0] ?? null)

	function msgOf(err: unknown): string {
		return (err as NimbusError).message ?? String(err)
	}

	async function load() {
		loading = true
		error = null
		try {
			items = await ipc.fetchNews()
		} catch (err) {
			// The feed is fetched from GitHub, so being offline is a normal
			// outcome rather than a bug: it must not look like a crash.
			error = msgOf(err)
		} finally {
			loading = false
		}
	}

	onMount(() => {
		void load()
	})

	function title(item: NewsItem): string {
		return ru ? item.titleRu : item.titleEn
	}

	function body(item: NewsItem): string {
		return ru ? item.bodyRu : item.bodyEn
	}

	/** Falls back to the raw string for a hand-written date we cannot parse. */
	function fmtDate(iso: string): string {
		const date = new Date(iso)
		if (Number.isNaN(date.getTime())) return iso
		return date.toLocaleDateString(locale(), {
			day: "numeric",
			month: "long",
			year: "numeric",
		})
	}

	async function openLink(url: string) {
		sound.play("click")
		try {
			await ipc.openUrl(url)
		} catch (err) {
			toasts.error(msgOf(err))
		}
	}
</script>

<div class="pane">
	<div class="bar">
		<span class="bar-label">{t("Обновления и анонсы Nimbus Client")}</span>
		{#if latestRelease && onwhatsnew}
			<button
				class="btn--sm"
				type="button"
				onclick={() => {
					sound.play("open")
					onwhatsnew?.()
				}}
			>
				<Icon name="sparkles" size={13} />
				{t("Что нового")}
			</button>
		{/if}
		<button
			class="btn--sm"
			type="button"
			disabled={loading}
			onclick={() => {
				sound.play("click")
				void load()
			}}
		>
			<Icon name="refresh" size={13} />
			{loading ? t("Обновление…") : t("Обновить")}
		</button>
	</div>

	{#if loading && items.length === 0}
		<div class="load" role="status" aria-live="polite">
			<span class="load-spinner" aria-hidden="true"></span>
			<span>{t("Загрузка новостей…")}</span>
		</div>
	{:else if error && items.length === 0}
		<EmptyState
			icon="wifiOff"
			title={t("Новости недоступны")}
			body={error}
			actionLabel={t("Повторить")}
			onaction={() => void load()}
		/>
	{:else if items.length === 0}
		<EmptyState
			icon="info"
			title={t("Пока ничего нет")}
			body={t("Как только появится новость, она отобразится здесь.")}
		/>
	{:else}
		{#if error}
			<p class="stale">{t("Показаны ранее загруженные новости: обновить ленту не удалось.")}</p>
		{/if}
		<ul class="feed">
			{#each items as item, i (item.id)}
				<li
					class="card anim-fade-up"
					style={`animation-delay:${Math.min(i, 8) * 40}ms`}
				>
					<div class="card-head">
						<span class="card-glyph" aria-hidden="true">
							<Icon name="sparkles" size={15} />
						</span>
						<div class="card-title">
							<span class="card-name">{title(item)}</span>
							<span class="card-date">{fmtDate(item.date)}</span>
						</div>
					</div>
					<p class="card-body">{body(item)}</p>
					{#if item.link}
						{@const link = item.link}
						<div class="card-foot">
							<button class="btn--sm" type="button" onclick={() => void openLink(link)}>
								<Icon name="globe" size={13} />
								{t("Подробнее")}
							</button>
						</div>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.pane {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
		width: 100%;
		max-width: 1080px;
		margin: 0 auto;
		padding: var(--sp-6);
	}

	.bar {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
	}

	.bar-label {
		flex: 1;
		min-width: 0;
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-caps);
		color: var(--text-tertiary);
	}

	.load {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--sp-3);
		padding: var(--sp-12) var(--sp-6);
		font-size: var(--fs-body);
		color: var(--text-tertiary);
	}

	.load-spinner {
		width: 15px;
		height: 15px;
		border-radius: var(--r-full);
		border: 2px solid var(--border-strong);
		border-top-color: var(--accent);
		animation: spinSlow 700ms linear infinite;
	}

	.stale {
		font-size: var(--fs-small);
		color: var(--warn);
	}

	.feed {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
		padding: var(--sp-4);
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-card);
	}

	.card-head {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
	}

	.card-glyph {
		flex: none;
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: var(--r-sm);
		color: var(--accent);
		background: var(--accent-soft);
	}

	.card-title {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.card-name {
		font-size: var(--fs-body);
		font-weight: var(--fw-semibold);
		color: var(--text-primary);
	}

	.card-date {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.card-body {
		font-size: var(--fs-body);
		line-height: 1.65;
		color: var(--text-secondary);
		white-space: pre-wrap;
		user-select: text;
		-webkit-user-select: text;
	}

	.card-foot {
		display: flex;
		justify-content: flex-start;
	}

	@media (prefers-reduced-motion: reduce) {
		.load-spinner {
			animation: none;
		}
	}
</style>
