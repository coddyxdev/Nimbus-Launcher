<script lang="ts">
	import type { ChangelogEntry } from "$lib/changelog"
	import { i18n, locale, t, tf } from "$lib/i18n.svelte"
	import { sound } from "$lib/sound.svelte"
	import Icon from "./Icon.svelte"

	let {
		entries,
		version,
		onclose,
	}: {
		/** Newest first; the dialog is not rendered when this is empty. */
		entries: ChangelogEntry[]
		version: string
		onclose: () => void
	} = $props()

	const ru = $derived(i18n.current === "ru")

	function title(entry: ChangelogEntry): string {
		return ru ? entry.titleRu : entry.titleEn
	}

	function items(entry: ChangelogEntry): string[] {
		return ru ? entry.itemsRu : entry.itemsEn
	}

	function fmtDate(iso: string): string {
		const date = new Date(iso)
		if (Number.isNaN(date.getTime())) return iso
		return date.toLocaleDateString(locale(), {
			day: "numeric",
			month: "long",
			year: "numeric",
		})
	}

	function close() {
		sound.play("click")
		onclose()
	}

	// The dialog owns Escape while it is open; App's own handler never sees it.
	function onKeyDown(e: KeyboardEvent) {
		if (e.key !== "Escape") return
		e.preventDefault()
		e.stopPropagation()
		onclose()
	}
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="scrim anim-fade-in">
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<div
		class="sheet anim-scale-in"
		role="dialog"
		aria-modal="true"
		aria-label={t("Что нового")}
	>
		<div class="head">
			<span class="glyph" aria-hidden="true">
				<Icon name="sparkles" size={17} />
			</span>
			<div class="head-text">
				<span class="head-title">{t("Что нового")}</span>
				<span class="head-meta">{tf("Версия {0}", version)}</span>
			</div>
			<button
				class="btn--icon"
				type="button"
				aria-label={t("Закрыть")}
				onclick={close}
			>
				<Icon name="close" size={14} />
			</button>
		</div>

		<div class="body">
			{#each entries as entry (entry.version)}
				<section class="release">
					<div class="release-head">
						<span class="release-version tnum">{entry.version}</span>
						<span class="release-title">{title(entry)}</span>
						<span class="release-date">{fmtDate(entry.date)}</span>
					</div>
					<ul class="release-list">
						{#each items(entry) as line}
							<li class="release-item">
								<span class="release-dot" aria-hidden="true"></span>
								<span>{line}</span>
							</li>
						{/each}
					</ul>
				</section>
			{/each}
		</div>

		<div class="foot">
			<button class="btn btn--play" type="button" onclick={close}>
				{t("Понятно")}
			</button>
		</div>
	</div>
</div>

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: var(--z-modal);
		display: grid;
		place-items: center;
		padding: var(--sp-6);
		background: rgba(0, 0, 0, 0.5);
		backdrop-filter: blur(3px);
	}

	.sheet {
		display: flex;
		flex-direction: column;
		width: 100%;
		max-width: 520px;
		max-height: 80vh;
		border-radius: var(--r-xl);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-overlay);
		overflow: hidden;
	}

	.head {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-4);
		border-bottom: 1px solid var(--border-subtle);
	}

	.glyph {
		flex: none;
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		border-radius: var(--r-md);
		color: var(--accent);
		background: var(--accent-soft);
	}

	.head-text {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.head-title {
		font-family: var(--font-display);
		font-size: var(--fs-title);
		font-weight: var(--fw-semibold);
		letter-spacing: var(--tracking-tighter);
		color: var(--text-primary);
	}

	.head-meta {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: var(--sp-5);
		padding: var(--sp-4);
	}

	.release {
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
	}

	.release-head {
		display: flex;
		align-items: baseline;
		gap: var(--sp-2);
	}

	.release-version {
		flex: none;
		padding: 2px var(--sp-2);
		border-radius: var(--r-sm);
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		color: var(--accent);
		background: var(--accent-soft);
	}

	.release-title {
		flex: 1;
		min-width: 0;
		font-size: var(--fs-body);
		font-weight: var(--fw-medium);
		color: var(--text-primary);
	}

	.release-date {
		flex: none;
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.release-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
	}

	.release-item {
		display: flex;
		align-items: flex-start;
		gap: var(--sp-3);
		font-size: var(--fs-body);
		line-height: 1.6;
		color: var(--text-secondary);
	}

	.release-dot {
		flex: none;
		width: 5px;
		height: 5px;
		margin-top: 8px;
		border-radius: var(--r-full);
		background: var(--accent);
	}

	.foot {
		flex: none;
		display: flex;
		justify-content: flex-end;
		padding: var(--sp-4);
		border-top: 1px solid var(--border-subtle);
	}
</style>
