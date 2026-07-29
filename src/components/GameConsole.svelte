<script lang="ts">
	/**
	 * Game output console.
	 *
	 * The parent owns the line buffer (it is filled from batched `game:output`
	 * events); this component renders, filters, virtualises and scrolls it.
	 *
	 * Rendering is windowed: only the rows that fit the viewport (plus a small
	 * overscan) exist in the DOM. Rows never wrap (`white-space: pre`), so every
	 * row has the same fixed height and the offsets can be computed arithmetically
	 * instead of measured. That keeps 5000 buffered lines at ~30 live nodes.
	 */
	import { sound } from "$lib/sound.svelte"
	import Icon from "./Icon.svelte"

	export type ConsoleEntry = { line: string; stream: "out" | "err" }

	let {
		lines = [],
		onclear,
		onexport,
	}: {
		lines: ConsoleEntry[]
		onclear: () => void
		/** Receives exactly what is on screen, so filters apply to the export. */
		onexport?: (text: string) => void
	} = $props()

	/** Must match `.console-row { height }` in the stylesheet. */
	const ROW_H = 18
	const OVERSCAN = 8

	let el = $state<HTMLDivElement | null>(null)
	let filter = $state("")
	let errorsOnly = $state(false)
	/** Autoscroll only while the user is parked at the bottom. */
	let stickToBottom = $state(true)
	let scrollTop = $state(0)
	let viewportH = $state(240)

	const filtered = $derived.by(() => {
		const needle = filter.trim().toLowerCase()
		if (!needle && !errorsOnly) return lines
		return lines.filter((c) => {
			if (errorsOnly && c.stream !== "err") return false
			if (!needle) return true
			return c.line.toLowerCase().includes(needle)
		})
	})

	const totalH = $derived(filtered.length * ROW_H)
	const startIndex = $derived(
		Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN),
	)
	const endIndex = $derived(
		Math.min(
			filtered.length,
			Math.ceil((scrollTop + viewportH) / ROW_H) + OVERSCAN,
		),
	)
	const visible = $derived(filtered.slice(startIndex, endIndex))
	const offsetY = $derived(startIndex * ROW_H)

	function scrollToBottom() {
		const node = el
		if (!node) return
		requestAnimationFrame(() => {
			node.scrollTop = node.scrollHeight
			scrollTop = node.scrollTop
		})
	}

	function onScroll() {
		const node = el
		if (!node) return
		scrollTop = node.scrollTop
		const distance = node.scrollHeight - node.scrollTop - node.clientHeight
		stickToBottom = distance < 40
	}

	$effect(() => {
		// Re-run whenever the visible row count changes (new output, new filter).
		void filtered.length
		if (stickToBottom) scrollToBottom()
	})

	function visibleText() {
		return filtered.map((c) => c.line).join("\n")
	}

	function copyAll() {
		sound.play("click")
		void navigator.clipboard.writeText(visibleText())
	}
</script>

<div class="console anim-fade-up">
	<div class="bar" role="toolbar" aria-label="Управление консолью">
		<span class="bar-title">
			<Icon name="terminal" size={13} />
			Консоль
			<span class="bar-count tnum">{filtered.length} из {lines.length}</span>
		</span>
		<div class="bar-tools">
			{#if !stickToBottom}
				<button
					class="btn--sm"
					type="button"
					title="К последним строкам"
					onclick={() => {
						sound.play("click")
						stickToBottom = true
						scrollToBottom()
					}}
				>
					<Icon name="chevronDown" size={13} strokeWidth={2} />
					Вниз
				</button>
			{/if}
			<button
				class="btn--sm"
				class:btn--on={errorsOnly}
				type="button"
				aria-pressed={errorsOnly}
				title="Только ошибки"
				onclick={() => {
					sound.play("toggle")
					errorsOnly = !errorsOnly
				}}
			>
				Ошибки
			</button>
			<div class="filter">
				<span class="filter-icon" aria-hidden="true"><Icon name="filter" size={12} /></span>
				<input
					class="filter-input"
					type="text"
					placeholder="Фильтр"
					aria-label="Фильтр консоли"
					bind:value={filter}
				/>
			</div>
			<button class="btn--sm" type="button" title="Копировать видимое" onclick={copyAll}>
				<Icon name="copy" size={13} />
			</button>
			{#if onexport}
				<button
					class="btn--sm"
					type="button"
					title="Сохранить лог в файл"
					disabled={filtered.length === 0}
					onclick={() => {
						sound.play("click")
						onexport?.(visibleText())
					}}
				>
					<Icon name="download" size={13} />
				</button>
			{/if}
			<button
				class="btn--sm"
				type="button"
				title="Очистить"
				onclick={() => {
					sound.play("click")
					onclear()
				}}
			>
				<Icon name="trash" size={13} />
			</button>
		</div>
	</div>

	<div class="body" bind:this={el} bind:clientHeight={viewportH} onscroll={onScroll}>
		{#if lines.length === 0}
			<span class="idle">
				<span class="idle-pip" aria-hidden="true"></span>
				Ожидание вывода игры…
			</span>
		{:else if filtered.length === 0}
			<span class="idle">Ничего не найдено по фильтру</span>
		{:else}
			<div class="spacer" style="height: {totalH}px">
				<div class="window" style="transform: translateY({offsetY}px)">
					{#each visible as entry, i (startIndex + i)}
						<div class="line" class:line--err={entry.stream === "err"}>{entry.line}</div>
					{/each}
				</div>
			</div>
		{/if}
	</div>
</div>

<style>
	.console {
		flex: none;
		display: flex;
		flex-direction: column;
		height: 264px;
		max-height: 46vh;
		background: var(--bg-inset);
		border-top: 1px solid var(--border);
	}

	.bar {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-3);
		padding: var(--sp-2) var(--sp-4);
		background: var(--bg-surface);
		border-bottom: 1px solid var(--border-subtle);
	}

	.bar-title {
		display: inline-flex;
		align-items: center;
		gap: var(--sp-2);
		font-size: var(--fs-small);
		font-weight: var(--fw-semibold);
		color: var(--text-secondary);
	}

	.bar-count {
		font-size: var(--fs-micro);
		font-weight: var(--fw-regular);
		color: var(--text-tertiary);
	}

	.bar-tools {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
	}

	.filter {
		position: relative;
		display: flex;
		align-items: center;
	}

	.filter-icon {
		position: absolute;
		left: var(--sp-2);
		display: grid;
		place-items: center;
		color: var(--text-tertiary);
		pointer-events: none;
	}

	.filter-input {
		width: 132px;
		height: 28px;
		padding: 0 var(--sp-2) 0 26px;
		border: 0;
		border-radius: var(--r-sm);
		background: var(--bg-inset);
		color: var(--text-primary);
		font-size: var(--fs-small);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
		user-select: text;
		-webkit-user-select: text;
		transition: box-shadow var(--dur-fast) var(--ease-out);
	}
	.filter-input::placeholder {
		color: var(--text-tertiary);
	}
	.filter-input:focus {
		outline: none;
		box-shadow:
			inset 0 0 0 1px var(--accent-border), 0 0 0 3px var(--accent-soft);
	}

	.body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		font-family: var(--font-mono);
		font-size: var(--fs-micro);
		color: var(--text-secondary);
		user-select: text;
		-webkit-user-select: text;
	}

	.spacer {
		position: relative;
	}

	.window {
		position: absolute;
		inset: 0 0 auto 0;
	}

	/* Virtualisation contract: the row height MUST stay in sync with ROW_H in
	   the script, so rows never wrap and offsets can be computed, not measured. */
	.line {
		height: 18px;
		line-height: 18px;
		padding: 0 var(--sp-4);
		white-space: pre;
		overflow: hidden;
		text-overflow: ellipsis;
		border-left: 2px solid transparent;
	}
	.line:hover {
		background: rgba(255, 255, 255, 0.03);
	}

	.line--err {
		color: var(--danger);
		border-left-color: var(--danger);
		background: var(--danger-soft);
	}

	.idle {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		padding: var(--sp-4);
		font-family: var(--font-sans);
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}

	.idle-pip {
		width: 6px;
		height: 6px;
		border-radius: var(--r-full);
		background: var(--text-disabled);
		animation: pulseRing 2s var(--ease-out) infinite;
	}

	@media (prefers-reduced-motion: reduce) {
		.idle-pip {
			animation: none;
		}
	}
</style>
