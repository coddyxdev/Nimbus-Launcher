<script lang="ts">
	import { t, tf } from "$lib/i18n.svelte"
	/**
	 * Command palette (Ctrl+K).
	 *
	 * One keyboard surface for everything that otherwise needs a mouse trip:
	 * jumping to a build, launching or stopping it, opening its folder, and the
	 * global actions. The list is a flat array of commands so filtering and
	 * arrow-key navigation stay trivial.
	 */
	import type { Instance } from "$lib/ipc"
	import { sound } from "$lib/sound.svelte"
	import type { IconName } from "$lib/icons"
	import Icon from "./Icon.svelte"

	let {
		open = $bindable(false),
		instances,
		runningIds = [],
		onselect,
		onplay,
		onstop,
		onfolder,
		oncreate,
		onsettings,
		onthemes,
	}: {
		open?: boolean
		instances: Instance[]
		runningIds?: string[]
		onselect: (id: string) => void
		onplay: (id: string) => void
		onstop: (id: string) => void
		onfolder: (id: string) => void
		oncreate: () => void
		onsettings: () => void
		onthemes: () => void
	} = $props()

	type Command = {
		id: string
		label: string
		hint: string
		icon: IconName
		run: () => void
	}

	let query = $state("")
	let cursor = $state(0)
	let inputEl = $state<HTMLInputElement | null>(null)

	const running = $derived(new Set(runningIds))

	const commands = $derived.by<Command[]>(() => {
		const list: Command[] = [
			{
				id: "new",
				label: t("Новая сборка"),
				hint: "Ctrl + N",
				icon: "plus",
				run: oncreate,
			},
			{
				id: "themes",
				label: t("Оформление"),
				hint: "Ctrl + Shift + T",
				icon: "sparkles",
				run: onthemes,
			},
			{
				id: "settings",
				label: t("Настройки"),
				hint: "Ctrl + ,",
				icon: "settings",
				run: onsettings,
			},
		]

		for (const inst of instances) {
			const live = running.has(inst.id)
			list.push({
				id: `open:${inst.id}`,
				label: inst.name,
				hint: tf("Открыть · {0}", inst.minecraftVersion ?? inst.versionId),
				icon: "cube",
				run: () => onselect(inst.id),
			})
			list.push({
				id: live ? `stop:${inst.id}` : `play:${inst.id}`,
				label: live ? tf("Остановить «{0}»", inst.name) : tf("Играть в «{0}»", inst.name),
				hint: live ? t("Игра запущена") : t("Запустить сборку"),
				icon: live ? "stop" : "play",
				run: () => (live ? onstop(inst.id) : onplay(inst.id)),
			})
			list.push({
				id: `folder:${inst.id}`,
				label: tf("Папка «{0}»", inst.name),
				hint: t("Открыть в проводнике"),
				icon: "folder",
				run: () => onfolder(inst.id),
			})
		}

		return list
	})

	const matches = $derived.by(() => {
		const needle = query.trim().toLowerCase()
		if (!needle) return commands.slice(0, 40)
		return commands
			.filter((c) => c.label.toLowerCase().includes(needle))
			.slice(0, 40)
	})

	// Reset the state every time the palette opens, and focus the field.
	$effect(() => {
		if (!open) return
		query = ""
		cursor = 0
		// The input only exists after the overlay renders.
		requestAnimationFrame(() => inputEl?.focus())
	})

	// Keep the highlight inside the (shrinking) result list while typing.
	$effect(() => {
		if (cursor >= matches.length) cursor = Math.max(0, matches.length - 1)
	})

	function close() {
		open = false
	}

	function run(command: Command) {
		close()
		sound.play("click")
		command.run()
	}

	function onKeyDown(e: KeyboardEvent) {
		if (e.key === "Escape") {
			e.preventDefault()
			e.stopPropagation()
			close()
			return
		}
		if (e.key === "ArrowDown") {
			e.preventDefault()
			cursor = matches.length === 0 ? 0 : (cursor + 1) % matches.length
		} else if (e.key === "ArrowUp") {
			e.preventDefault()
			cursor =
				matches.length === 0 ? 0 : (cursor - 1 + matches.length) % matches.length
		} else if (e.key === "Enter") {
			e.preventDefault()
			const target = matches[cursor]
			if (target) run(target)
		}
	}
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="scrim anim-fade-in" onclick={close}>
		<div
			class="palette anim-pop-in"
			role="dialog"
			tabindex="-1"
			aria-modal="true"
			aria-label={t("Палитра команд")}
			onclick={(e) => e.stopPropagation()}
			onkeydown={onKeyDown}
		>
			<div class="field">
				<span class="field-icon" aria-hidden="true"><Icon name="search" size={15} /></span>
				<input
					class="field-input"
					type="text"
					placeholder={t("Найти сборку или команду…")}
					aria-label={t("Поиск команды")}
					spellcheck="false"
					bind:this={inputEl}
					bind:value={query}
				/>
				<kbd class="kbd">Esc</kbd>
			</div>

			<div class="results" role="listbox" aria-label={t("Результаты")}>
				{#each matches as command, i (command.id)}
					<button
						class="item"
						class:item--on={i === cursor}
						type="button"
						role="option"
						aria-selected={i === cursor}
						onmouseenter={() => (cursor = i)}
						onclick={() => run(command)}
					>
						<span class="item-glyph"><Icon name={command.icon} size={15} /></span>
						<span class="item-text">
							<span class="item-label">{command.label}</span>
							<span class="item-hint">{command.hint}</span>
						</span>
					</button>
				{/each}

				{#if matches.length === 0}
					<div class="void">{t("Ничего не найдено")}</div>
				{/if}
			</div>

			<div class="foot">
				<span><kbd class="kbd">↑</kbd><kbd class="kbd">↓</kbd> {t("выбор")}</span>
				<span><kbd class="kbd">↵</kbd> {t("выполнить")}</span>
			</div>
		</div>
	</div>
{/if}

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: var(--z-modal);
		display: flex;
		justify-content: center;
		align-items: flex-start;
		padding: 12vh var(--sp-6) var(--sp-6);
		background: var(--bg-scrim);
		backdrop-filter: blur(6px);
	}

	.palette {
		width: 100%;
		max-width: 560px;
		display: flex;
		flex-direction: column;
		border-radius: var(--r-xl);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-overlay);
		overflow: hidden;
	}

	.field {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-4);
		border-bottom: 1px solid var(--border-subtle);
	}

	.field-icon {
		display: grid;
		place-items: center;
		color: var(--text-tertiary);
	}

	.field-input {
		flex: 1;
		min-width: 0;
		border: 0;
		background: transparent;
		font-size: var(--fs-title);
		color: var(--text-primary);
		user-select: text;
		-webkit-user-select: text;
	}
	.field-input:focus {
		outline: none;
	}
	.field-input::placeholder {
		color: var(--text-tertiary);
	}

	.kbd {
		padding: 1px 5px;
		border-radius: var(--r-xs);
		font-family: var(--font-sans);
		font-size: 10px;
		color: var(--text-tertiary);
		background: var(--bg-surface);
		box-shadow: inset 0 0 0 1px var(--border);
	}

	.results {
		max-height: 44vh;
		overflow-y: auto;
		padding: var(--sp-2);
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.item {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		width: 100%;
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-md);
		text-align: left;
		color: var(--text-secondary);
		transition: background var(--dur-instant) var(--ease-out);
	}
	.item--on {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.item-glyph {
		flex: none;
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: var(--r-sm);
		color: var(--text-tertiary);
		background: var(--bg-surface);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
	}
	.item--on .item-glyph {
		color: var(--accent);
		box-shadow: inset 0 0 0 1px var(--accent-border);
	}

	.item-text {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.item-label {
		font-size: var(--fs-body);
		font-weight: var(--fw-medium);
		color: inherit;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.item-hint {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.void {
		padding: var(--sp-8);
		text-align: center;
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}

	.foot {
		display: flex;
		gap: var(--sp-4);
		padding: var(--sp-2) var(--sp-4);
		border-top: 1px solid var(--border-subtle);
		background: var(--bg-surface);
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}
	.foot span {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
</style>
