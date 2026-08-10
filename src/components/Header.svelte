<script lang="ts">
	import { t } from "$lib/i18n.svelte"
	import type { Snippet } from "svelte"
	import type { IconName } from "$lib/icons"
	import Icon from "./Icon.svelte"

	let {
		title,
		meta = "",
		initials = "",
		icon,
		chips = [],
		status = "idle",
		actions,
	}: {
		title: string
		meta?: string
		/** Two-letter monogram for the selected build. */
		initials?: string
		/** Glyph shown instead of a monogram on non-instance views. */
		icon?: IconName
		/** Short factual tags: loader, version, size. */
		chips?: string[]
		status?: "idle" | "starting" | "running"
		actions?: Snippet
	} = $props()
</script>

<div class="hero">
	<div class="identity">
		{#if initials}
			<span class="avatar" class:avatar--live={status !== "idle"}>
				<span class="avatar-text">{initials}</span>
			</span>
		{:else if icon}
			<span class="avatar avatar--glyph">
				<Icon name={icon} size={18} />
			</span>
		{/if}

		<div class="text">
			<h1 class="title">{title}</h1>
			<div class="sub">
				{#if status === "running"}
					<span class="state">
						<span class="pip" aria-hidden="true"></span>
						{t("Запущена")}
					</span>
				{:else if status === "starting"}
					<span class="state state--pending">
						<span class="pip" aria-hidden="true"></span>
						{t("Запуск…")}
					</span>
				{/if}
				{#each chips as chip (chip)}
					<span class="tag">{chip}</span>
				{/each}
				{#if meta}
					<span class="meta tnum">{meta}</span>
				{/if}
			</div>
		</div>
	</div>

	{#if actions}
		<div class="actions">{@render actions()}</div>
	{/if}
</div>

<style>
	.hero {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
		min-height: var(--header-h);
		padding: var(--sp-4) var(--sp-6);
		background: var(--bg-surface);
		border-bottom: 1px solid var(--border-subtle);
		/* A soft top light so the header reads as a distinct plane. */
		background-image: var(--gradient-radial-glow);
	}

	.identity {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		min-width: 0;
	}

	.avatar {
		position: relative;
		flex: none;
		display: grid;
		place-items: center;
		width: 42px;
		height: 42px;
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-sm);
		color: var(--text-secondary);
	}

	.avatar-text {
		font-family: var(--font-display);
		font-size: var(--fs-title);
		font-weight: var(--fw-semibold);
		letter-spacing: var(--tracking-tighter);
		color: var(--text-primary);
	}

	.avatar--live {
		box-shadow:
			inset 0 0 0 1px var(--accent-border), var(--edge-top),
			0 6px 18px -10px var(--accent-glow);
	}
	.avatar--live .avatar-text {
		color: var(--accent);
	}

	.avatar--glyph {
		color: var(--text-tertiary);
	}

	.text {
		min-width: 0;
	}

	.title {
		font-family: var(--font-display);
		font-size: var(--fs-display);
		font-weight: var(--fw-semibold);
		line-height: var(--lh-tight);
		letter-spacing: var(--tracking-tighter);
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.sub {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--sp-2);
		margin-top: 5px;
		min-height: 18px;
	}

	.tag {
		display: inline-flex;
		align-items: center;
		height: 19px;
		padding: 0 var(--sp-2);
		border-radius: var(--r-xs);
		font-size: var(--fs-micro);
		font-weight: var(--fw-medium);
		color: var(--text-secondary);
		box-shadow: inset 0 0 0 1px var(--border);
	}

	.meta {
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}

	.state {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		height: 19px;
		padding: 0 var(--sp-2) 0 6px;
		border-radius: var(--r-full);
		font-size: var(--fs-micro);
		font-weight: var(--fw-medium);
		color: var(--accent);
		background: var(--accent-soft);
		box-shadow: inset 0 0 0 1px var(--accent-border);
	}

	.pip {
		width: 6px;
		height: 6px;
		border-radius: var(--r-full);
		background: currentColor;
		animation: pulseRing 2s var(--ease-out) infinite;
	}

	.state--pending {
		color: var(--warn);
		background: var(--warn-soft);
		box-shadow: inset 0 0 0 1px rgba(226, 163, 54, 0.32);
	}

	.actions {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--sp-2);
	}

	@media (prefers-reduced-motion: reduce) {
		.pip {
			animation: none;
		}
	}
</style>
