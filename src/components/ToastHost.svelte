<script lang="ts">
	/**
	 * Renders the global toast queue in the bottom-right corner.
	 * Mounted once from App.svelte; everything else just calls `toasts.*`.
	 */
	import { fly } from "svelte/transition"
	import { sound } from "$lib/sound.svelte"
	import { toasts, type ToastKind } from "$lib/toast.svelte"

	const LABEL: Record<ToastKind, string> = {
		info: "Инфо",
		success: "Готово",
		error: "Ошибка",
	}

	// Play a sound exactly once per new toast that appears.
	const announced = new Set<number>()
	$effect(() => {
		for (const toast of toasts.items) {
			if (announced.has(toast.id)) continue
			announced.add(toast.id)
			sound.play(toast.kind === "error" ? "error" : toast.kind === "success" ? "success" : "toast")
		}
	})
</script>

<div class="host" aria-live="polite" aria-atomic="false">
	{#each toasts.items as toast (toast.id)}
		<div
			class="toast toast--{toast.kind}"
			role={toast.kind === "error" ? "alert" : "status"}
			in:fly={{ y: 12, duration: 260 }}
			out:fly={{ x: 20, duration: 180 }}
		>
			<span class="dot" aria-hidden="true"></span>
			<div class="text">
				<span class="label">{LABEL[toast.kind]}</span>
				<span class="body">{toast.text}</span>
			</div>
			<button
				class="close"
				type="button"
				aria-label="Закрыть уведомление"
				onclick={() => toasts.dismiss(toast.id)}
			>
				×
			</button>
		</div>
	{/each}
</div>

<style>
	.host {
		position: fixed;
		right: var(--sp-5);
		bottom: var(--sp-5);
		z-index: var(--z-toast);
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		max-width: 380px;
		pointer-events: none;
	}

	.toast {
		display: flex;
		align-items: flex-start;
		gap: var(--sp-3);
		padding: var(--sp-3) var(--sp-3) var(--sp-3) var(--sp-4);
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-overlay);
		pointer-events: auto;
	}

	.dot {
		flex: none;
		width: 7px;
		height: 7px;
		margin-top: 6px;
		border-radius: var(--r-full);
		background: var(--text-tertiary);
	}

	.text {
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}

	.label {
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-caps);
		color: var(--text-tertiary);
	}

	.body {
		font-size: var(--fs-small);
		line-height: 1.5;
		color: var(--text-primary);
		overflow-wrap: anywhere;
	}

	.close {
		flex: none;
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border-radius: var(--r-sm);
		font-size: 15px;
		line-height: 1;
		color: var(--text-tertiary);
		transition:
			background var(--dur-fast) var(--ease-out),
			color var(--dur-fast) var(--ease-out);
	}
	.close:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.toast--success .dot {
		background: var(--accent);
		box-shadow: 0 0 0 3px var(--accent-soft);
	}
	.toast--success .label {
		color: var(--accent);
	}

	.toast--error .dot {
		background: var(--danger);
		box-shadow: 0 0 0 3px var(--danger-soft);
	}
	.toast--error .label {
		color: var(--danger);
	}

	.toast--info .dot {
		background: var(--text-secondary);
	}
</style>
