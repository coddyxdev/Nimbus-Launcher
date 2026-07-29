<script lang="ts">
	import Icon from "./Icon.svelte"
	import type { IconName } from "$lib/icons"

	let {
		icon,
		title,
		body,
		actionLabel = "",
		onaction,
		tone = "neutral",
	}: {
		icon: IconName
		title: string
		body: string
		actionLabel?: string
		onaction?: () => void
		tone?: "neutral" | "danger"
	} = $props()
</script>

<div class="empty anim-fade-up">
	<span class="glyph" class:glyph--danger={tone === "danger"}>
		<Icon name={icon} size={22} strokeWidth={1.5} />
	</span>
	<p class="title">{title}</p>
	<p class="body">{body}</p>
	{#if actionLabel && onaction}
		<button class="btn" class:btn--play={tone !== "danger"} type="button" onclick={onaction}>
			{actionLabel}
		</button>
	{/if}
</div>

<style>
	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-14) var(--sp-6);
		text-align: center;
	}

	.glyph {
		display: grid;
		place-items: center;
		width: 54px;
		height: 54px;
		margin-bottom: var(--sp-2);
		border-radius: var(--r-xl);
		color: var(--text-tertiary);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-card);
	}

	.glyph--danger {
		color: var(--danger);
		background: var(--danger-soft);
		box-shadow: inset 0 0 0 1px rgba(242, 85, 90, 0.28), var(--edge-top);
	}

	.title {
		font-family: var(--font-display);
		font-size: var(--fs-title);
		font-weight: var(--fw-semibold);
		letter-spacing: var(--tracking-tighter);
		color: var(--text-primary);
	}

	.body {
		max-width: 400px;
		font-size: var(--fs-body);
		line-height: 1.6;
		color: var(--text-tertiary);
	}

	.empty :global(.btn) {
		margin-top: var(--sp-3);
	}
</style>
