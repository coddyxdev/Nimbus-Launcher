<script lang="ts">
	import Icon from "./Icon.svelte"
	import { sound } from "$lib/sound.svelte"
	import { t } from "$lib/i18n.svelte"

	let {
		page,
		totalPages,
		disabled = false,
		onchange,
	}: {
		/** 1-based current page. */
		page: number
		totalPages: number
		disabled?: boolean
		onchange: (page: number) => void
	} = $props()

	/** Page numbers to render, with `null` standing in for an ellipsis. */
	const items = $derived.by(() => {
		const total = Math.max(1, totalPages)
		if (total <= 7) return Array.from({ length: total }, (_, i) => i + 1)

		const out: (number | null)[] = [1]
		const start = Math.max(2, page - 1)
		const end = Math.min(total - 1, page + 1)
		if (start > 2) out.push(null)
		for (let p = start; p <= end; p++) out.push(p)
		if (end < total - 1) out.push(null)
		out.push(total)
		return out
	})

	function go(p: number) {
		if (p === page || p < 1 || p > totalPages || disabled) return
		sound.play("click")
		onchange(p)
	}
</script>

{#if totalPages > 1}
	<nav class="pager" aria-label={t("Страницы")}>
		<button
			class="pager-btn pager-btn--nav"
			type="button"
			disabled={disabled || page <= 1}
			aria-label={t("Предыдущая страница")}
			onclick={() => go(page - 1)}
		>
			<Icon name="chevronLeft" size={14} />
		</button>

		{#each items as item, i (i)}
			{#if item === null}
				<span class="pager-ellipsis" aria-hidden="true">…</span>
			{:else}
				<button
					class="pager-btn tnum"
					class:pager-btn--active={item === page}
					type="button"
					disabled={disabled && item !== page}
					aria-current={item === page ? "page" : undefined}
					onclick={() => go(item)}
				>
					{item}
				</button>
			{/if}
		{/each}

		<button
			class="pager-btn pager-btn--nav"
			type="button"
			disabled={disabled || page >= totalPages}
			aria-label={t("Следующая страница")}
			onclick={() => go(page + 1)}
		>
			<Icon name="chevronRight" size={14} />
		</button>
	</nav>
{/if}

<style>
	.pager {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 4px;
		padding: var(--sp-3) 0;
	}

	.pager-btn {
		display: grid;
		place-items: center;
		min-width: 28px;
		height: 28px;
		padding: 0 6px;
		border-radius: var(--r-sm);
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-secondary);
		background: transparent;
		transition:
			background var(--dur-fast) var(--ease-out),
			color var(--dur-fast) var(--ease-out);
	}

	.pager-btn:hover:not(:disabled) {
		background: var(--bg-raised);
		color: var(--text-primary);
	}

	.pager-btn:disabled {
		color: var(--text-disabled);
		cursor: default;
	}

	.pager-btn--active {
		color: var(--accent);
		background: var(--accent-soft);
		box-shadow: inset 0 0 0 1px var(--accent-border);
	}

	.pager-btn--nav {
		color: var(--text-tertiary);
	}

	.pager-ellipsis {
		display: grid;
		place-items: center;
		min-width: 20px;
		height: 28px;
		color: var(--text-disabled);
	}
</style>
