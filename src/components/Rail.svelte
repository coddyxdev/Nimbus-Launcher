<script lang="ts">
	import type { Instance } from "$lib/ipc"
	import { locale, t } from "$lib/i18n.svelte"
	import { sound } from "$lib/sound.svelte"
	import Icon from "./Icon.svelte"

	export type RailAction =
		| "play"
		| "stop"
		| "rename"
		| "duplicate"
		| "favorite"
		| "folder"
		| "delete"

	let {
		instances,
		selectedId,
		view,
		runningIds = [],
		brokenIds = [],
		installing = false,
		onselect,
		oncreate,
		onsettings,
		onthemes,
		onnews,
		onaction,
	}: {
		instances: Instance[]
		selectedId: string | null
		view: "instance" | "settings" | "create" | "themes" | "news"
		/** Instances with a live game process; shown with a jade pip. */
		runningIds?: string[]
		/** Instances with incomplete files; shown with an amber marker. */
		brokenIds?: string[]
		/** Highlights the «new instance» row while a download is in flight. */
		installing?: boolean
		onselect: (id: string) => void
		oncreate: () => void
		onsettings: () => void
		onthemes: () => void
		onnews: () => void
		/** Right-click menu. When omitted, the context menu is not rendered. */
		onaction?: (id: string, action: RailAction) => void
	} = $props()

	const LOADER_BADGES: Record<string, string> = {
		fabric: "F",
		quilt: "Q",
		forge: "Fg",
		neoforge: "Nf",
		nimbus: "N",
	}

	const LOADER_NAMES: Record<string, string> = {
		fabric: "Fabric",
		quilt: "Quilt",
		forge: "Forge",
		neoforge: "NeoForge",
		nimbus: "Nimbus Client",
	}

	const COLLAPSE_KEY = "nimbus.sidebar.collapsed"

	const running = $derived(new Set(runningIds))
	const broken = $derived(new Set(brokenIds))

	let query = $state("")
	let collapsed = $state(readCollapsed())

	function readCollapsed(): boolean {
		try {
			return window.localStorage.getItem(COLLAPSE_KEY) === "1"
		} catch {
			return false
		}
	}

	function toggleCollapsed() {
		collapsed = !collapsed
		sound.play("tab")
		try {
			window.localStorage.setItem(COLLAPSE_KEY, collapsed ? "1" : "0")
		} catch {
			/* Private mode — the preference is simply not persisted. */
		}
	}

	/** Local filter over name, version and loader. Cheap enough to run inline. */
	const matched = $derived.by(() => {
		const q = query.trim().toLowerCase()
		if (!q) return instances
		return instances.filter((i) => {
			const loader = loaderName(i.loader).toLowerCase()
			return (
				i.name.toLowerCase().includes(q) ||
				(i.minecraftVersion ?? "").toLowerCase().includes(q) ||
				i.versionId.toLowerCase().includes(q) ||
				loader.includes(q)
			)
		})
	})

	/** Favourites float to the top; the rest keep their stored order. */
	const shown = $derived([
		...matched.filter((i) => i.favorite),
		...matched.filter((i) => !i.favorite),
	])

	const MENU_W = 200
	const MENU_H = 256

	let menu = $state<{ id: string; x: number; y: number } | null>(null)
	let menuEl = $state<HTMLDivElement | null>(null)

	const menuInstance = $derived(
		menu ? (instances.find((i) => i.id === menu!.id) ?? null) : null,
	)

	function openMenu(e: MouseEvent, id: string) {
		if (!onaction) return
		e.preventDefault()
		sound.play("open")
		menu = {
			id,
			x: Math.min(e.clientX, window.innerWidth - MENU_W - 8),
			y: Math.min(e.clientY, window.innerHeight - MENU_H - 8),
		}
	}

	function openMenuFromKeyboard(e: KeyboardEvent, id: string) {
		if (!onaction) return
		const wanted = e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")
		if (!wanted) return
		e.preventDefault()
		sound.play("open")
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
		menu = {
			id,
			x: Math.min(rect.right + 4, window.innerWidth - MENU_W - 8),
			y: Math.min(rect.top, window.innerHeight - MENU_H - 8),
		}
	}

	function closeMenu() {
		menu = null
	}

	function run(action: RailAction) {
		const target = menu?.id
		closeMenu()
		if (target && onaction) onaction(target, action)
	}

	// Focus the menu when it opens, and close it on outside interaction.
	$effect(() => {
		if (!menu || !menuEl) return
		const root = menuEl
		root.querySelector<HTMLElement>("button")?.focus()

		function onPointerDown(e: PointerEvent) {
			if (!root.contains(e.target as Node)) closeMenu()
		}
		function onKey(e: KeyboardEvent) {
			if (e.key === "Escape") {
				e.stopPropagation()
				closeMenu()
			}
		}

		window.addEventListener("pointerdown", onPointerDown, true)
		window.addEventListener("keydown", onKey, true)
		window.addEventListener("resize", closeMenu)
		return () => {
			window.removeEventListener("pointerdown", onPointerDown, true)
			window.removeEventListener("keydown", onKey, true)
			window.removeEventListener("resize", closeMenu)
		}
	})

	function onMenuKeyDown(e: KeyboardEvent) {
		if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return
		e.preventDefault()
		const nodes = Array.from(menuEl?.querySelectorAll<HTMLElement>("button") ?? [])
		if (nodes.length === 0) return
		const idx = nodes.indexOf(document.activeElement as HTMLElement)
		const next =
			e.key === "ArrowDown"
				? nodes[(idx + 1) % nodes.length]
				: nodes[(idx - 1 + nodes.length) % nodes.length]
		next?.focus()
	}

	function initials(name: string): string {
		const parts = name.trim().split(/[\s_-]+/).filter(Boolean)
		if (parts.length === 0) return "?"
		if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase()
		return (parts[0]![0]! + parts[1]![0]!).toUpperCase()
	}

	function loaderBadge(loader: string | null): string | null {
		return loader ? (LOADER_BADGES[loader] ?? null) : null
	}

	/** Resolved on render: reading the language keeps every caller reactive. */
	function loaderName(loader: string | null): string {
		const fallback = t("Vanilla")
		return loader ? (LOADER_NAMES[loader] ?? loader) : fallback
	}

	function formatDate(ts: number | null): string {
		if (!ts) return ""
		const date = new Date(ts * 1000)
		const dayDelta = Math.round((date.getTime() - Date.now()) / 86_400_000)
		if (Math.abs(dayDelta) < 7) {
			return new Intl.RelativeTimeFormat(locale(), { numeric: "auto" }).format(dayDelta, "day")
		}
		return date.toLocaleDateString(locale(), { day: "numeric", month: "short" })
	}

	function select(id: string) {
		if (id !== selectedId || view !== "instance") sound.play("tab")
		onselect(id)
	}
</script>

<nav class="sidebar" class:sidebar--collapsed={collapsed} aria-label={t("Сборки")}>
	<div class="head">
		{#if !collapsed}
			<span class="head-label">{t("Библиотека")}</span>
			<span class="head-count tnum">{instances.length}</span>
		{/if}
		<button
			class="collapse"
			type="button"
			aria-label={collapsed ? t("Развернуть панель") : t("Свернуть панель")}
			title={collapsed ? t("Развернуть панель") : t("Свернуть панель")}
			onclick={toggleCollapsed}
		>
			<span class="collapse-glyph" class:collapse-glyph--flipped={collapsed}>
				<Icon name="chevronRight" size={15} strokeWidth={1.8} />
			</span>
		</button>
	</div>

	{#if !collapsed}
		<div class="search">
			<span class="search-icon" aria-hidden="true">
				<Icon name="search" size={13} />
			</span>
			<input
				class="search-input"
				type="text"
				placeholder={t("Поиск сборки")}
				aria-label={t("Поиск сборки")}
				spellcheck="false"
				bind:value={query}
			/>
			{#if query}
				<button
					class="search-clear"
					type="button"
					aria-label={t("Очистить поиск")}
					onclick={() => (query = "")}
				>
					<Icon name="close" size={12} strokeWidth={2} />
				</button>
			{/if}
		</div>
	{/if}

	<div class="scroll">
		<ul class="list">
			{#each shown as instance, i (instance.id)}
				<li class="anim-item" style={`animation-delay:${Math.min(i, 10) * 26}ms`}>
					<button
						class="row"
						class:row--active={view === "instance" && instance.id === selectedId}
						class:row--menu={menu?.id === instance.id}
						type="button"
						aria-current={instance.id === selectedId ? "true" : undefined}
						aria-haspopup={onaction ? "menu" : undefined}
						onclick={() => select(instance.id)}
						onmouseenter={() => sound.play("hover")}
						oncontextmenu={(e) => openMenu(e, instance.id)}
						onkeydown={(e) => openMenuFromKeyboard(e, instance.id)}
					>
						<span class="tile" class:tile--broken={broken.has(instance.id)}>
							<span class="tile-text">{initials(instance.name)}</span>
							{#if loaderBadge(instance.loader)}
								<span
									class="tile-badge"
									class:badge--fabric={instance.loader === "fabric"}
									class:badge--quilt={instance.loader === "quilt"}
									class:badge--forge={instance.loader === "forge"}
									class:badge--neoforge={instance.loader === "neoforge"}
								>
									{loaderBadge(instance.loader)}
								</span>
							{/if}
							{#if broken.has(instance.id)}
								<span class="tile-warn" aria-hidden="true">
									<Icon name="alert" size={9} strokeWidth={2.4} />
								</span>
							{/if}
						</span>

						<span class="row-text">
							<span class="row-name">{instance.name}</span>
							<span class="row-meta">
								{loaderName(instance.loader)} · {instance.minecraftVersion}
								{#if instance.lastPlayed}
									· {formatDate(instance.lastPlayed)}
								{/if}
							</span>
						</span>

						{#if instance.favorite}
							<span class="row-star" title={t("В избранном")}>
								<Icon name="star" size={12} strokeWidth={1.7} />
							</span>
						{/if}

						{#if running.has(instance.id)}
							<span class="row-live" aria-hidden="true"></span>
						{/if}

						<span class="tip" role="presentation">
							<span class="tip-name">{instance.name}</span>
							<span class="tip-meta">
								{loaderName(instance.loader)} · {instance.minecraftVersion}
							</span>
						</span>
					</button>
				</li>
			{/each}

			{#if shown.length === 0}
				<li class="void">
					{#if instances.length === 0}
						Сборок пока нет
					{:else}
						Ничего не найдено
					{/if}
				</li>
			{/if}
		</ul>
	</div>

	<div class="foot">
		<button
			class="row row--create"
			class:row--active={view === "create"}
			class:row--busy={installing}
			type="button"
			onclick={() => {
				sound.play("open")
				oncreate()
			}}
			onmouseenter={() => sound.play("hover")}
		>
			<span class="tile tile--ghost">
				<Icon name="plus" size={17} strokeWidth={1.9} />
			</span>
			<span class="row-text">
				<span class="row-name">{installing ? t("Идёт установка…") : t("Новая сборка")}</span>
				<span class="row-meta">{installing ? t("загрузка файлов") : "Ctrl + N"}</span>
			</span>
			<span class="tip" role="presentation">
				<span class="tip-name">{installing ? t("Идёт установка…") : t("Новая сборка")}</span>
			</span>
		</button>

		<button
			class="row row--quiet"
			class:row--active={view === "news"}
			type="button"
			onclick={() => {
				sound.play("open")
				onnews()
			}}
			onmouseenter={() => sound.play("hover")}
		>
			<span class="tile tile--ghost">
				<Icon name="globe" size={17} />
			</span>
			<span class="row-text">
				<span class="row-name">{t("Новости")}</span>
				<span class="row-meta">{t("обновления проекта")}</span>
			</span>
			<span class="tip" role="presentation">
				<span class="tip-name">{t("Новости")}</span>
			</span>
		</button>

		<button
			class="row row--quiet"
			class:row--active={view === "themes"}
			type="button"
			onclick={() => {
				sound.play("open")
				onthemes()
			}}
			onmouseenter={() => sound.play("hover")}
		>
			<span class="tile tile--ghost">
				<Icon name="sparkles" size={17} />
			</span>
			<span class="row-text">
				<span class="row-name">{t("Оформление")}</span>
				<span class="row-meta">Ctrl + Shift + T</span>
			</span>
			<span class="tip" role="presentation">
				<span class="tip-name">{t("Оформление")}</span>
			</span>
		</button>

		<button
			class="row row--quiet"
			class:row--active={view === "settings"}
			type="button"
			onclick={() => {
				sound.play("open")
				onsettings()
			}}
			onmouseenter={() => sound.play("hover")}
		>
			<span class="tile tile--ghost">
				<Icon name="settings" size={17} />
			</span>
			<span class="row-text">
				<span class="row-name">{t("Настройки")}</span>
				<span class="row-meta">Ctrl + ,</span>
			</span>
			<span class="tip" role="presentation">
				<span class="tip-name">{t("Настройки")}</span>
			</span>
		</button>
	</div>
</nav>

{#if menu && menuInstance}
	<div
		class="menu anim-pop-in"
		role="menu"
		tabindex="-1"
		aria-label={`Действия: ${menuInstance.name}`}
		style={`left:${menu.x}px; top:${menu.y}px;`}
		bind:this={menuEl}
		onkeydown={onMenuKeyDown}
	>
		<div class="menu-head">{menuInstance.name}</div>
		{#if running.has(menuInstance.id)}
			<button class="menu-item" type="button" role="menuitem" onclick={() => run("stop")}>
				<Icon name="stop" size={14} />
				{t("Остановить")}
			</button>
		{:else}
			<button
				class="menu-item"
				type="button"
				role="menuitem"
				disabled={broken.has(menuInstance.id)}
				onclick={() => run("play")}
			>
				<Icon name="play" size={14} />
				{t("Играть")}
			</button>
		{/if}
		<button class="menu-item" type="button" role="menuitem" onclick={() => run("favorite")}>
		<Icon name="star" size={14} />
		{menuInstance.favorite ? t("Убрать из избранного") : t("В избранное")}
	</button>
	<button class="menu-item" type="button" role="menuitem" onclick={() => run("rename")}>
			<Icon name="edit" size={14} />
			{t("Переименовать")}
		</button>
		<button class="menu-item" type="button" role="menuitem" onclick={() => run("duplicate")}>
			<Icon name="copy" size={14} />
			{t("Дублировать")}
		</button>
		<button class="menu-item" type="button" role="menuitem" onclick={() => run("folder")}>
			<Icon name="folder" size={14} />
			{t("Папка игры")}
		</button>
		<div class="menu-sep"></div>
		<button
			class="menu-item menu-item--danger"
			type="button"
			role="menuitem"
			onclick={() => run("delete")}
		>
			<Icon name="trash" size={14} />
			{t("Удалить")}
		</button>
	</div>
{/if}

<style>
	/* ── Shell ───────────────────────────────────────────────── */

	.sidebar {
		position: relative;
		flex: none;
		display: flex;
		flex-direction: column;
		width: var(--rail-w);
		background: var(--bg-surface);
		border-right: 1px solid var(--border-subtle);
		transition: width var(--dur-slow) var(--ease-out);
	}
	.sidebar--collapsed {
		width: var(--rail-w-collapsed);
	}

	/* ── Head ────────────────────────────────────────────────── */

	.head {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		height: 44px;
		padding: 0 var(--sp-2) 0 var(--sp-4);
	}
	.sidebar--collapsed .head {
		justify-content: center;
		padding: 0;
	}

	.head-label {
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-caps);
		color: var(--text-tertiary);
	}

	.head-count {
		margin-right: auto;
		font-size: var(--fs-micro);
		color: var(--text-disabled);
	}

	.collapse {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: var(--r-sm);
		color: var(--text-tertiary);
		transition:
			background var(--dur-fast) var(--ease-out),
			color var(--dur-fast) var(--ease-out);
	}
	.collapse:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.collapse-glyph {
		display: block;
		transform: rotate(180deg);
		transition: transform var(--dur-slow) var(--ease-spring);
	}
	.collapse-glyph--flipped {
		transform: rotate(0deg);
	}

	/* ── Search ──────────────────────────────────────────────── */

	.search {
		position: relative;
		flex: none;
		display: flex;
		align-items: center;
		margin: 0 var(--sp-3) var(--sp-2);
	}

	.search-icon {
		position: absolute;
		left: var(--sp-3);
		display: grid;
		place-items: center;
		color: var(--text-tertiary);
		pointer-events: none;
	}

	.search-input {
		width: 100%;
		height: 32px;
		padding: 0 28px 0 32px;
		border: 0;
		border-radius: var(--r-md);
		background: var(--bg-inset);
		color: var(--text-primary);
		font-size: var(--fs-small);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
		user-select: text;
		-webkit-user-select: text;
		transition:
			box-shadow var(--dur-fast) var(--ease-out),
			background var(--dur-fast) var(--ease-out);
	}
	.search-input::placeholder {
		color: var(--text-tertiary);
	}
	.search-input:hover:not(:focus) {
		box-shadow: inset 0 0 0 1px var(--border);
	}
	.search-input:focus {
		outline: none;
		box-shadow:
			inset 0 0 0 1px var(--accent-border), 0 0 0 3px var(--accent-soft);
	}

	.search-clear {
		position: absolute;
		right: 6px;
		display: grid;
		place-items: center;
		width: 20px;
		height: 20px;
		border-radius: var(--r-full);
		color: var(--text-tertiary);
		transition:
			background var(--dur-fast) var(--ease-out),
			color var(--dur-fast) var(--ease-out);
	}
	.search-clear:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	/* ── List ────────────────────────────────────────────────── */

	.scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overflow-x: hidden;
		padding: 0 var(--sp-2) var(--sp-2);
	}

	.list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.anim-item {
		animation: fadeInUp var(--dur-base) var(--ease-out) both;
	}

	.void {
		padding: var(--sp-5) var(--sp-3);
		text-align: center;
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}
	.sidebar--collapsed .void {
		display: none;
	}

	/* ── Row ─────────────────────────────────────────────────── */

	.row {
		position: relative;
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		width: 100%;
		padding: var(--sp-2);
		border-radius: var(--r-md);
		text-align: left;
		color: var(--text-secondary);
		transition:
			background var(--dur-fast) var(--ease-out),
			color var(--dur-fast) var(--ease-out);
	}
	.sidebar--collapsed .row {
		justify-content: center;
		padding: var(--sp-2) 0;
	}

	.row::before {
		content: "";
		position: absolute;
		left: -2px;
		top: 50%;
		width: 3px;
		height: 0;
		border-radius: var(--r-full);
		background: var(--accent);
		transform: translateY(-50%);
		transition: height var(--dur-base) var(--ease-spring);
	}

	.row:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	.row:active {
		background: var(--bg-active);
	}

	.row--active {
		background: var(--bg-raised);
		color: var(--text-primary);
		box-shadow: var(--edge-ring), var(--edge-top);
	}
	.row--active::before {
		height: 18px;
	}
	.row--menu {
		background: var(--bg-active);
	}

	.row-text {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}
	.sidebar--collapsed .row-text {
		display: none;
	}

	.row-name {
		font-size: var(--fs-body);
		font-weight: var(--fw-medium);
		line-height: 1.3;
		color: inherit;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.row-meta {
		font-size: var(--fs-micro);
		line-height: 1.4;
		color: var(--text-tertiary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.row-star {
		flex: none;
		display: grid;
		place-items: center;
		color: var(--accent);
	}
	.sidebar--collapsed .row-star {
		position: absolute;
		top: 4px;
		left: 10px;
	}

	.row-live {
		flex: none;
		width: 7px;
		height: 7px;
		margin-right: 2px;
		border-radius: var(--r-full);
		background: var(--accent);
		animation: pulseRing 2.2s var(--ease-out) infinite;
	}
	.sidebar--collapsed .row-live {
		position: absolute;
		top: 6px;
		right: 12px;
		margin: 0;
	}

	/* ── Tile ────────────────────────────────────────────────── */

	.tile {
		position: relative;
		flex: none;
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		border-radius: var(--r-md);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top);
		transition:
			box-shadow var(--dur-fast) var(--ease-out),
			transform var(--dur-base) var(--ease-spring);
	}
	.row:hover .tile {
		transform: translateY(-1px);
	}
	.row:active .tile {
		transform: scale(0.96);
	}
	.row--active .tile {
		box-shadow: inset 0 0 0 1px var(--border-strong), var(--edge-top);
	}

	.tile-text {
		font-family: var(--font-display);
		font-size: var(--fs-small);
		font-weight: var(--fw-semibold);
		letter-spacing: var(--tracking-tighter);
		color: var(--text-secondary);
	}
	.row--active .tile-text,
	.row:hover .tile-text {
		color: var(--text-primary);
	}

	.tile--ghost {
		background: transparent;
		box-shadow: inset 0 0 0 1px var(--border-subtle);
		color: var(--text-tertiary);
	}
	.row:hover .tile--ghost {
		color: var(--text-primary);
		box-shadow: inset 0 0 0 1px var(--border);
	}

	.tile--broken {
		box-shadow: inset 0 0 0 1px rgba(226, 163, 54, 0.3);
	}

	.row--busy .tile--ghost {
		color: var(--accent);
		box-shadow: inset 0 0 0 1px var(--accent-border);
	}

	/* Loader monogram, bottom-right of the tile. */
	.tile-badge {
		position: absolute;
		right: -3px;
		bottom: -3px;
		display: grid;
		place-items: center;
		min-width: 15px;
		height: 15px;
		padding: 0 3px;
		border-radius: var(--r-xs);
		font-size: 9px;
		font-weight: var(--fw-bold);
		line-height: 1;
		color: var(--text-secondary);
		background: var(--bg-active);
		box-shadow: 0 0 0 2px var(--bg-surface);
	}
	.row--active .tile-badge {
		box-shadow: 0 0 0 2px var(--bg-raised);
	}
	.badge--fabric {
		color: #d5b071;
	}
	.badge--quilt {
		color: #b28ad8;
	}
	.badge--forge {
		color: #8fa4c4;
	}
	.badge--neoforge {
		color: #d99168;
	}

	.tile-warn {
		position: absolute;
		top: -3px;
		right: -3px;
		display: grid;
		place-items: center;
		width: 14px;
		height: 14px;
		border-radius: var(--r-full);
		color: var(--warn);
		background: var(--bg-surface);
		box-shadow: 0 0 0 1px rgba(226, 163, 54, 0.4);
	}

	/* ── Foot ────────────────────────────────────────────────── */

	.foot {
		flex: none;
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: var(--sp-2);
		border-top: 1px solid var(--border-subtle);
	}

	.row--quiet .row-name {
		font-weight: var(--fw-regular);
	}

	/* ── Collapsed tooltips ──────────────────────────────────── */

	.tip {
		position: absolute;
		left: calc(100% + 10px);
		top: 50%;
		z-index: var(--z-overlay);
		display: none;
		flex-direction: column;
		gap: 1px;
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-md);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--shadow-pop);
		white-space: nowrap;
		pointer-events: none;
		opacity: 0;
		transform: translate(-4px, -50%);
		transition:
			opacity var(--dur-fast) var(--ease-out),
			transform var(--dur-base) var(--ease-out);
	}
	.sidebar--collapsed .row:hover .tip,
	.sidebar--collapsed .row:focus-visible .tip {
		display: flex;
		opacity: 1;
		transform: translate(0, -50%);
	}

	.tip-name {
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-primary);
	}
	.tip-meta {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	/* ── Context menu ────────────────────────────────────────── */

	.menu {
		position: fixed;
		z-index: var(--z-modal);
		min-width: 200px;
		padding: var(--sp-1);
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-overlay);
		transform-origin: top left;
	}

	.menu-head {
		padding: var(--sp-2) var(--sp-3) var(--sp-2);
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-caps);
		color: var(--text-tertiary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.menu-item {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		width: 100%;
		min-height: 32px;
		padding: 0 var(--sp-3);
		border-radius: var(--r-sm);
		font-size: var(--fs-body);
		color: var(--text-secondary);
		text-align: left;
		transition:
			background var(--dur-instant) var(--ease-out),
			color var(--dur-instant) var(--ease-out);
	}
	.menu-item:hover:not(:disabled) {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	.menu-item:disabled {
		color: var(--text-disabled);
		cursor: default;
	}
	.menu-item--danger {
		color: var(--danger);
	}
	.menu-item--danger:hover:not(:disabled) {
		background: var(--danger-soft);
		color: var(--danger);
	}

	.menu-sep {
		height: 1px;
		margin: var(--sp-1) var(--sp-2);
		background: var(--border-subtle);
	}

	@media (prefers-reduced-motion: reduce) {
		.row-live {
			animation: none;
		}
		.anim-item {
			animation: none;
		}
	}
</style>
