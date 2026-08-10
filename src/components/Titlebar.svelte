<script lang="ts">
	import { t } from "$lib/i18n.svelte"
	import { getCurrentWindow } from "@tauri-apps/api/window"
	import { sound } from "$lib/sound.svelte"
	import Icon from "./Icon.svelte"

	let { subtitle = "" }: { subtitle?: string } = $props()

	let maximized = $state(false)
	const appWindow = getCurrentWindow()

	async function windowAction(action: () => Promise<unknown>) {
		try {
			await action()
		} catch (error) {
			console.error("Window action failed", error)
		}
	}

	$effect(() => {
		let disposed = false
		let unlisten: (() => void) | undefined

		const sync = async () => {
			try {
				maximized = await appWindow.isMaximized()
			} catch (error) {
				console.error("Failed to read window state", error)
			}
		}

		void sync()
		void appWindow.onResized(() => void sync()).then((fn) => {
			if (disposed) {
				fn()
			} else {
				unlisten = fn
			}
		})

		return () => {
			disposed = true
			unlisten?.()
		}
	})
</script>

<!--
	data-tauri-drag-region turns the empty area into the OS drag handle, which
	keeps Windows snap layouts and aero-snap working on a decorations:false
	window. Buttons must sit outside that region or dragging swallows clicks.
-->
<header class="titlebar" data-tauri-drag-region>
	<div class="brand" data-tauri-drag-region>
		<span class="mark-wrap">
			<img class="mark" src="/logo.png" alt="" aria-hidden="true" draggable="false" />
		</span>
		<span class="name">Nimbus</span>
		{#if subtitle}
			<span class="version tnum">{subtitle}</span>
		{/if}
	</div>

	<div class="controls">
		<button
			class="ctl"
			type="button"
			aria-label={t("Свернуть")}
			onclick={() => {
				sound.play("click")
				void windowAction(() => appWindow.minimize())
			}}
		>
			<Icon name="minimize" size={13} strokeWidth={1.7} />
		</button>
		<button
			class="ctl"
			type="button"
			aria-label={maximized ? t("Восстановить") : t("Развернуть")}
			onclick={() => {
				sound.play("click")
				void windowAction(() => appWindow.toggleMaximize())
			}}
		>
			<Icon name={maximized ? "restore" : "maximize"} size={13} strokeWidth={1.7} />
		</button>
		<button
			class="ctl ctl--close"
			type="button"
			aria-label={t("Закрыть")}
			onclick={() => {
				sound.play("stop")
				void windowAction(() => appWindow.close())
			}}
		>
			<Icon name="close" size={13} strokeWidth={1.7} />
		</button>
	</div>
</header>

<style>
	.titlebar {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		height: var(--titlebar-h);
		padding-left: var(--sp-4);
		background: var(--bg-surface);
		border-bottom: 1px solid var(--border-subtle);
		/* Keeps the chrome visually attached to the window edge. */
		box-shadow: var(--edge-top);
	}

	.brand {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		min-width: 0;
		pointer-events: none;
	}

	.mark-wrap {
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border-radius: var(--r-sm);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top);
		overflow: hidden;
	}

	.mark {
		width: 16px;
		height: 16px;
		object-fit: contain;
		display: block;
	}

	.name {
		font-family: var(--font-display);
		font-size: var(--fs-body);
		font-weight: var(--fw-semibold);
		letter-spacing: var(--tracking-tighter);
		color: var(--text-primary);
	}

	.version {
		padding: 1px var(--sp-2);
		border-radius: var(--r-full);
		font-size: 10px;
		font-weight: var(--fw-medium);
		line-height: 16px;
		color: var(--text-tertiary);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
	}

	.controls {
		display: flex;
		align-items: stretch;
		height: 100%;
	}

	/* Windows-native hit targets: full-height, no radius, no gap. */
	.ctl {
		display: grid;
		place-items: center;
		width: 44px;
		height: 100%;
		color: var(--text-tertiary);
		transition:
			background var(--dur-fast) var(--ease-out),
			color var(--dur-fast) var(--ease-out);
	}
	.ctl:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	.ctl:active {
		background: var(--bg-active);
	}
	.ctl--close:hover {
		background: #e5484d;
		color: #fff;
	}
	.ctl--close:active {
		background: #c93c40;
		color: #fff;
	}
	.ctl:focus-visible {
		outline-offset: -3px;
		border-radius: 0;
	}
</style>
