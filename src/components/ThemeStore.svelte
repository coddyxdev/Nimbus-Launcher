<script lang="ts">
	import { t, tf } from "$lib/i18n.svelte"
	import Icon from "./Icon.svelte"
	import {
		ACCENTS,
		BASE_VARS,
		CUSTOM_ACCENT_ID,
		PRESETS,
		SYSTEM_ID,
		appearance,
		presetToCss,
		readCssVar,
		type CustomTheme,
		type ThemeBase,
		type ThemePreset,
	} from "$lib/themes.svelte"
	import { MAX_IMAGE_MIB, MAX_VIDEO_MIB, background } from "$lib/background.svelte"
	import {
		DEFAULT_ID,
		DEFAULT_SCALE,
		DEFAULT_UI_FAMILY,
		FONTS,
		fonts,
		isInstalled,
		MAX_SCALE,
		MIN_SCALE,
		stackOf,
		type FontKind,
	} from "$lib/fonts.svelte"

	type Tab = "themes" | "accents" | "custom" | "fonts" | "background"
	type Draft = { id: string | null; name: string; base: ThemeBase; css: string }

	let tab = $state<Tab>("themes")
	let filter = $state<"all" | "dark" | "light">("all")
	let draft = $state<Draft | null>(null)
	let notice = $state<{ tone: "ok" | "err"; text: string } | null>(null)
	let hex = $state(appearance.accentHex)
	let importer = $state<HTMLInputElement | null>(null)
	let noticeTimer: ReturnType<typeof setTimeout> | null = null

	const visible = $derived(
		PRESETS.filter((p) => {
			if (filter === "all") return true
			if (p.id === SYSTEM_ID) return false
			return p.base === filter
		}),
	)

	function say(tone: "ok" | "err", text: string): void {
		notice = { tone, text }
		if (noticeTimer) clearTimeout(noticeTimer)
		noticeTimer = setTimeout(() => (notice = null), 4000)
	}

	// ── Fonts ─────────────────────────────────────────────────

	let fontKind = $state<"all" | FontKind>("all")
	let fontQuery = $state("")
	let customFont = $state("")

	const KIND_LABELS: Record<FontKind, string> = {
		sans: "Без засечек",
		serif: "С засечками",
		mono: "Моноширинный",
		display: "Декоративный",
	}

	// Offering a font the machine cannot render would be a dead option, so the
	// catalogue is filtered by an actual measurement first. Results are cached
	// inside the fonts module, so this stays cheap on re-runs.
	const installedFonts = $derived(FONTS.filter((f) => isInstalled(f.family)))

	const uiFonts = $derived(
		installedFonts.filter((f) => {
			if (fontKind !== "all" && f.kind !== fontKind) return false
			const needle = fontQuery.trim().toLowerCase()
			return needle === "" || f.family.toLowerCase().includes(needle)
		}),
	)

	const monoFonts = $derived(installedFonts.filter((f) => f.kind === "mono"))

	function applyCustomFont(): void {
		const family = customFont.trim()
		if (!family) return
		if (!isInstalled(family)) {
			say("err", tf("Шрифт «{0}» не найден в системе", family))
			return
		}
		fonts.setUi(family)
		customFont = ""
		say("ok", tf("Шрифт «{0}» применён", family))
	}

	function resetFonts(): void {
		fonts.reset()
		say("ok", t("Стандартные шрифты возвращены"))
	}

	// ── Previews ────────────────────────────────────────────────────────────

	// ── Background ─────────────────────────────────────────────────────

	async function pickBackground(): Promise<void> {
		const failure = await background.pick()
		if (failure) {
			say("err", failure)
			return
		}
		// A cancelled picker reports no failure and changes nothing, so only
		// confirm when a file actually landed.
		if (background.active) say("ok", t("Фон обновлён"))
	}

	async function removeBackground(): Promise<void> {
		await background.remove()
		say("ok", t("Фон убран"))
	}

	// ── Previews ──────────────────────────────────────────────────────

	function accentHexFor(id: string, base: ThemeBase): string {
		if (id === CUSTOM_ACCENT_ID) return appearance.accentHex
		const found = ACCENTS.find((a) => a.id === id)
		if (!found) return base === "light" ? "#12a05f" : "#3ecf8e"
		return base === "light" ? found.light : found.dark
	}

	function swatchStyle(canvas: string, surface: string, raised: string, text: string, dim: string, accent: string, base: ThemeBase): string {
		const border = base === "light" ? "rgba(16,16,22,0.12)" : "rgba(255,255,255,0.09)"
		return [
			`--pv-canvas:${canvas}`,
			`--pv-surface:${surface}`,
			`--pv-raised:${raised}`,
			`--pv-text:${text}`,
			`--pv-dim:${dim}`,
			`--pv-accent:${accent}`,
			`--pv-border:${border}`,
		].join(";")
	}

	function presetStyle(p: ThemePreset): string {
		const base: ThemeBase = p.id === SYSTEM_ID ? appearance.base : p.base
		const v = p.vars ?? BASE_VARS[base]
		return swatchStyle(v.canvas, v.surface, v.raised, v.text, v.text3, accentHexFor(p.accent, base), base)
	}

	function customStyle(c: CustomTheme): string {
		const fb = BASE_VARS[c.base]
		return swatchStyle(
			readCssVar(c.css, "bg-canvas") ?? fb.canvas,
			readCssVar(c.css, "bg-surface") ?? fb.surface,
			readCssVar(c.css, "bg-raised") ?? fb.raised,
			readCssVar(c.css, "text-primary") ?? fb.text,
			readCssVar(c.css, "text-tertiary") ?? fb.text3,
			readCssVar(c.css, "accent") ?? accentHexFor(appearance.accentId, c.base),
			c.base,
		)
	}

	// ── Actions ────────────────────────────────────────────────────────────

	function pickTheme(id: string): void {
		appearance.setTheme(id)
	}

	function pickAccent(id: string): void {
		appearance.setAccent(id)
	}

	function applyHex(): void {
		const value = hex.trim()
		if (!/^#?[0-9a-fA-F]{3}([0-9a-fA-F]{3})?$/.test(value)) {
			say("err", t("Нужен цвет в формате #RRGGBB"))
			return
		}
		const normalized = value.startsWith("#") ? value : `#${value}`
		hex = normalized
		appearance.setAccent(CUSTOM_ACCENT_ID, normalized)
		say("ok", t("Свой акцент применён"))
	}

	function startNew(): void {
		draft = {
			id: null,
			name: t("Моя тема"),
			base: appearance.base,
			css: presetToCss(appearance.preset ?? PRESETS[1]!),
		}
		tab = "custom"
	}

	function startEdit(c: CustomTheme): void {
		draft = { id: c.id, name: c.name, base: c.base, css: c.css }
		tab = "custom"
	}

	function saveDraft(apply: boolean): void {
		if (!draft) return
		try {
			const saved = appearance.saveCustom({
				id: draft.id ?? undefined,
				name: draft.name,
				base: draft.base,
				css: draft.css,
			})
			draft = { id: saved.id, name: saved.name, base: saved.base, css: saved.css }
			if (apply) appearance.setTheme(`custom:${saved.id}`)
			say("ok", apply ? t("Тема сохранена и применена") : t("Тема сохранена"))
		} catch (err) {
			say("err", err instanceof Error ? err.message : t("Не удалось сохранить тему"))
		}
	}

	function removeCustom(c: CustomTheme): void {
		appearance.removeCustom(c.id)
		if (draft?.id === c.id) draft = null
		say("ok", t("Тема удалена"))
	}

	async function copyExport(): Promise<void> {
		if (appearance.customs.length === 0) {
			say("err", t("Пока нечего экспортировать"))
			return
		}
		try {
			await navigator.clipboard.writeText(appearance.exportCustoms())
			say("ok", t("JSON с темами скопирован в буфер"))
		} catch {
			say("err", t("Буфер обмена недоступен"))
		}
	}

	async function ingest(file: File): Promise<void> {
		const text = await file.text()
		if (file.name.toLowerCase().endsWith(".json")) {
			try {
				const count = appearance.importCustoms(text)
				say("ok", tf("Добавлено тем: {0}", count))
			} catch (err) {
				say("err", err instanceof Error ? err.message : t("Файл не прочитан"))
			}
			return
		}
		draft = {
			id: draft?.id ?? null,
			name: draft?.name ?? file.name.replace(/\.css$/i, ""),
			base: draft?.base ?? appearance.base,
			css: text,
		}
		say("ok", t("CSS загружен в редактор"))
	}

	async function onDrop(event: DragEvent): Promise<void> {
		event.preventDefault()
		const file = event.dataTransfer?.files?.[0]
		if (file) await ingest(file)
	}

	async function onPickFile(event: Event): Promise<void> {
		const input = event.currentTarget as HTMLInputElement
		const file = input.files?.[0]
		if (file) await ingest(file)
		input.value = ""
	}
</script>

<div class="store">
	<div class="store__head">
		<div class="tabs" role="tablist" aria-label={t("Разделы оформления")}>
			<button type="button" role="tab" aria-selected={tab === "themes"} class="tab" class:tab--on={tab === "themes"} onclick={() => (tab = "themes")}>
				<Icon name="sparkles" size={15} /> {t("Темы")}
			</button>
			<button type="button" role="tab" aria-selected={tab === "accents"} class="tab" class:tab--on={tab === "accents"} onclick={() => (tab = "accents")}>
				<Icon name="image" size={15} /> {t("Акценты")}
			</button>
			<button type="button" role="tab" aria-selected={tab === "custom"} class="tab" class:tab--on={tab === "custom"} onclick={() => (tab = "custom")}>
				<Icon name="edit" size={15} /> {t("Свои темы")}
				{#if appearance.customs.length > 0}<span class="count">{appearance.customs.length}</span>{/if}
			</button>
			<button type="button" role="tab" aria-selected={tab === "fonts"} class="tab" class:tab--on={tab === "fonts"} onclick={() => (tab = "fonts")}>
				<Icon name="fileText" size={15} /> {t("Шрифты")}
				{#if fonts.ui !== DEFAULT_UI_FAMILY || fonts.mono !== DEFAULT_ID || fonts.scale !== DEFAULT_SCALE}<span class="count">1</span>{/if}
			</button>
			<button type="button" role="tab" aria-selected={tab === "background"} class="tab" class:tab--on={tab === "background"} onclick={() => (tab = "background")}>
				<Icon name="image" size={15} /> {t("Фон")}
				{#if background.active}<span class="count">1</span>{/if}
			</button>
		</div>

		<div class="current">
			<span class="current__dot"></span>
			<span class="current__text">{t(appearance.themeName)}</span>
		</div>
	</div>

	{#if notice}
		<div class="notice" class:notice--err={notice.tone === "err"}>
			<Icon name={notice.tone === "err" ? "alert" : "check"} size={15} />
			<span>{notice.text}</span>
		</div>
	{/if}

	{#if tab === "themes"}
		<div class="filters">
			<button type="button" class="pill" class:pill--on={filter === "all"} onclick={() => (filter = "all")}>{t("Все")}</button>
			<button type="button" class="pill" class:pill--on={filter === "dark"} onclick={() => (filter = "dark")}>
				<Icon name="moon" size={14} /> {t("Тёмные")}
			</button>
			<button type="button" class="pill" class:pill--on={filter === "light"} onclick={() => (filter = "light")}>
				<Icon name="sun" size={14} /> {t("Светлые")}
			</button>
			<span class="filters__spacer"></span>
			<button type="button" class="ghost" onclick={startNew}>
				<Icon name="plus" size={14} /> {t("Своя тема")}
			</button>
		</div>

		<div class="grid">
			{#each visible as p (p.id)}
				<button type="button" class="card" class:card--on={appearance.themeId === p.id} onclick={() => pickTheme(p.id)}>
					<div class="pv" style={presetStyle(p)}>
						<div class="pv__rail">
							<span class="pv__dot"></span>
							<span class="pv__line"></span>
							<span class="pv__line pv__line--short"></span>
							<span class="pv__line"></span>
						</div>
						<div class="pv__body">
							<div class="pv__bar">
								<span class="pv__title"></span>
								<span class="pv__cta"></span>
							</div>
							<div class="pv__tiles">
								<span></span><span></span><span></span><span></span>
							</div>
						</div>
						{#if appearance.themeId === p.id}
							<span class="pv__check"><Icon name="check" size={13} /></span>
						{/if}
					</div>
					<div class="card__foot">
						<div class="card__name">
							{t(p.name)}
							<span class="badge">
								{#if p.id === SYSTEM_ID}{t("авто")}{:else if p.base === "dark"}{t("тёмная")}{:else}{t("светлая")}{/if}
							</span>
						</div>
						<div class="card__blurb">{t(p.blurb)}</div>
					</div>
				</button>
			{/each}
		</div>
	{:else if tab === "accents"}
		<p class="hint">{t("Акцент живёт отдельно от темы: любой цвет сочетается с любой темой, а оттенки наведения и свечения считаются автоматически.")}</p>
		<div class="accents">
			{#each ACCENTS as a (a.id)}
				<button type="button" class="accent" class:accent--on={appearance.accentId === a.id} data-accent={a.id} onclick={() => pickAccent(a.id)}>
					<span class="accent__chip">
						{#if appearance.accentId === a.id}<Icon name="check" size={14} />{/if}
					</span>
					<span class="accent__label">{t(a.label)}</span>
				</button>
			{/each}
		</div>

		<div class="custom-accent">
			<div class="custom-accent__head">{t("Свой цвет")}</div>
			<div class="custom-accent__row">
				<input class="color" type="color" aria-label={t("Выбрать цвет")} bind:value={hex} />
				<input class="text" type="text" spellcheck="false" aria-label={t("HEX цвета")} bind:value={hex} />
				<button type="button" class="primary" onclick={applyHex}>{t("Применить")}</button>
				{#if appearance.accentId === CUSTOM_ACCENT_ID}<span class="badge badge--on">{t("активен")}</span>{/if}
			</div>
		</div>
	{:else if tab === "custom"}
		<div class="filters">
			<button type="button" class="ghost" onclick={startNew}>
				<Icon name="plus" size={14} /> {t("Новая тема")}
			</button>
			<button type="button" class="ghost" onclick={() => importer?.click()}>
				<Icon name="upload" size={14} /> {t("Импорт файла")}
			</button>
			<button type="button" class="ghost" onclick={copyExport}>
				<Icon name="copy" size={14} /> {t("Экспорт в буфер")}
			</button>
			<input class="file" type="file" accept=".css,.json" bind:this={importer} onchange={onPickFile} />
		</div>

		{#if appearance.customs.length > 0}
			<div class="grid">
				{#each appearance.customs as c (c.id)}
					<div class="card card--static" class:card--on={appearance.themeId === `custom:${c.id}`}>
						<div class="pv" style={customStyle(c)}>
							<div class="pv__rail">
								<span class="pv__dot"></span>
								<span class="pv__line"></span>
								<span class="pv__line pv__line--short"></span>
							</div>
							<div class="pv__body">
								<div class="pv__bar"><span class="pv__title"></span><span class="pv__cta"></span></div>
								<div class="pv__tiles"><span></span><span></span><span></span><span></span></div>
							</div>
						</div>
						<div class="card__foot">
							<div class="card__name">
								{c.name}
								<span class="badge">{c.base === "dark" ? t("тёмная") : t("светлая")}</span>
							</div>
							<div class="card__actions">
								<button type="button" class="mini" onclick={() => pickTheme(`custom:${c.id}`)}>{t("Применить")}</button>
								<button type="button" class="mini" onclick={() => startEdit(c)}>{t("Изменить")}</button>
								<button type="button" class="mini mini--danger" onclick={() => removeCustom(c)}>{t("Удалить")}</button>
							</div>
						</div>
					</div>
				{/each}
			</div>
		{:else if !draft}
			<div class="empty">
				<Icon name="sparkles" size={22} />
				<div class="empty__title">{t("Своих тем пока нет")}</div>
				<div class="empty__text">{t("Создайте тему на основе текущей или перетащите файл .css в редактор.")}</div>
			</div>
		{/if}

		{#if draft}
			<div class="editor">
				<div class="editor__row">
					<input class="text text--grow" type="text" placeholder={t("Название темы")} aria-label={t("Название темы")} bind:value={draft.name} />
					<div class="seg">
						<button type="button" class="seg__btn" class:seg__btn--on={draft.base === "dark"} onclick={() => draft && (draft.base = "dark")}>
							<Icon name="moon" size={14} /> {t("Тёмная основа")}
						</button>
						<button type="button" class="seg__btn" class:seg__btn--on={draft.base === "light"} onclick={() => draft && (draft.base = "light")}>
							<Icon name="sun" size={14} /> {t("Светлая основа")}
						</button>
					</div>
				</div>

				<textarea
					class="code"
					spellcheck="false"
					aria-label={t("CSS темы")}
					placeholder={":root { --bg-canvas: #101015; }"}
					bind:value={draft.css}
					ondragover={(e) => e.preventDefault()}
					ondrop={onDrop}
				></textarea>

				<div class="editor__foot">
					<span class="editor__hint">{t("Перетащите сюда файл .css — он подставится автоматически")}</span>
					<span class="filters__spacer"></span>
					<button type="button" class="ghost" onclick={() => (draft = null)}>{t("Закрыть")}</button>
					<button type="button" class="ghost" onclick={() => saveDraft(false)}>{t("Сохранить")}</button>
					<button type="button" class="primary" onclick={() => saveDraft(true)}>{t("Сохранить и применить")}</button>
				</div>
			</div>
		{/if}
	{:else if tab === "fonts"}
		<p class="hint">
			{t(
				"Шрифт применяется ко всему лаунчеру сразу. В списке только те шрифты, которые действительно установлены в системе — остальные скрыты, чтобы нельзя было выбрать пустоту.",
			)}
		</p>

		<div class="filters">
			<button type="button" class="pill" class:pill--on={fontKind === "all"} onclick={() => (fontKind = "all")}>
				{t("Все")} <span class="count">{installedFonts.length}</span>
			</button>
			<button type="button" class="pill" class:pill--on={fontKind === "sans"} onclick={() => (fontKind = "sans")}>{t("Без засечек")}</button>
			<button type="button" class="pill" class:pill--on={fontKind === "serif"} onclick={() => (fontKind = "serif")}>{t("С засечками")}</button>
			<button type="button" class="pill" class:pill--on={fontKind === "mono"} onclick={() => (fontKind = "mono")}>{t("Моноширинные")}</button>
			<button type="button" class="pill" class:pill--on={fontKind === "display"} onclick={() => (fontKind = "display")}>{t("Декоративные")}</button>
			<input
				class="font-search"
				type="text"
				placeholder={t("Поиск шрифта")}
				aria-label={t("Поиск шрифта")}
				bind:value={fontQuery}
			/>
		</div>

		<div class="font-grid">
			<button
				type="button"
				class="font-card"
				class:font-card--on={fonts.ui === DEFAULT_ID}
				onclick={() => fonts.setUi(DEFAULT_ID)}
			>
				<span class="font-card__head">
					<span class="font-card__name">{t("Стандартный")}</span>
					{#if fonts.ui === DEFAULT_ID}<Icon name="check" size={14} />{/if}
				</span>
				<span class="font-card__sample">Aa Бб Cc 123</span>
				<span class="font-card__kind">{t("Как задумано дизайном")}</span>
			</button>

			{#each uiFonts as f (f.id)}
				<button
					type="button"
					class="font-card"
					class:font-card--on={fonts.ui === f.family}
					style={`--sample:${stackOf(f)}`}
					onclick={() => fonts.setUi(f.family)}
				>
					<span class="font-card__head">
						<span class="font-card__name">{f.family}</span>
						{#if fonts.ui === f.family}<Icon name="check" size={14} />{/if}
					</span>
					<span class="font-card__sample font-card__sample--own">Aa Бб Cc 123</span>
					<span class="font-card__kind">
						{t(KIND_LABELS[f.kind])}{#if f.family === DEFAULT_UI_FAMILY} · {t("по умолчанию")}{/if}
					</span>
				</button>
			{/each}
		</div>

		{#if uiFonts.length === 0}
			<div class="empty">
				<span class="empty__title">{t("Ничего не найдено")}</span>
				<span class="empty__text">{t("Попробуйте другой запрос или снимите фильтр.")}</span>
			</div>
		{/if}

		<div class="font-section">
			<span class="font-section__title">{t("Размер шрифта")}</span>
			<p class="hint">
				{t(
					"Ползунок меняет размер всего текста в лаунчере — от компактного до крупного. Выбранный шрифт при этом остаётся прежним.",
				)}
			</p>
			<div class="fs-row">
				<button
					type="button"
					class="fs-step"
					aria-label={t("Мельче")}
					disabled={fonts.scale <= MIN_SCALE}
					onclick={() => fonts.setScale(fonts.scale - 5)}
				>A</button>
				<input
					class="fs-range"
					type="range"
					min={MIN_SCALE}
					max={MAX_SCALE}
					step="5"
					value={fonts.scale}
					aria-label={t("Размер шрифта")}
					style={`--fill:${((fonts.scale - MIN_SCALE) / (MAX_SCALE - MIN_SCALE)) * 100}%`}
					oninput={(e) => fonts.setScale(Number(e.currentTarget.value))}
				/>
				<button
					type="button"
					class="fs-step fs-step--lg"
					aria-label={t("Крупнее")}
					disabled={fonts.scale >= MAX_SCALE}
					onclick={() => fonts.setScale(fonts.scale + 5)}
				>A</button>
				<span class="fs-value tnum">{fonts.scale}%</span>
				<button
					type="button"
					class="pill"
					disabled={fonts.scale === DEFAULT_SCALE}
					onclick={() => fonts.setScale(DEFAULT_SCALE)}
				>{t("Обычный размер")}</button>
			</div>
			<p class="fs-preview">
				{t("Так выглядит текст интерфейса: заголовки, подписи и кнопки меняются вместе.")}
			</p>
		</div>

		<div class="font-section">
			<span class="font-section__title">{t("Шрифт консоли и кода")}</span>
			<div class="font-chips">
				<button type="button" class="pill" class:pill--on={fonts.mono === DEFAULT_ID} onclick={() => fonts.setMono(DEFAULT_ID)}>
					{t("Стандартный")}
				</button>
				{#each monoFonts as f (f.id)}
					<button
						type="button"
						class="pill"
						class:pill--on={fonts.mono === f.family}
						style={`--sample:${stackOf(f)}`}
						onclick={() => fonts.setMono(f.family)}
					>
						<span class="font-chip">{f.family}</span>
					</button>
				{/each}
			</div>
		</div>

		<div class="font-section">
			<span class="font-section__title">{t("Свой шрифт")}</span>
			<p class="hint">{t("Впишите точное название любого шрифта, установленного в системе, — например, из папки Windows Fonts.")}</p>
			<div class="font-custom">
				<input
					class="font-input"
					type="text"
					placeholder={t("Название шрифта")}
					aria-label={t("Название шрифта")}
					bind:value={customFont}
					onkeydown={(e) => {
						if (e.key === "Enter") applyCustomFont()
					}}
				/>
				<button type="button" class="primary" onclick={applyCustomFont}>{t("Применить")}</button>
				<button type="button" class="ghost" onclick={resetFonts}>{t("Вернуть стандартные")}</button>
			</div>
		</div>
	{:else}
		<p class="hint">{t("Своё фото или короткий клип станет фоном всего лаунчера. Файл копируется в папку лаунчера, поэтому оригинал потом можно спокойно переместить или удалить.")}</p>

		<div class="bg-grid">
			<div class="bg-preview" class:bg-preview--empty={!background.active}>
				{#if background.src}
					{#key background.src}
						{#if background.kind === "video"}
							<!-- svelte-ignore a11y_media_has_caption -->
							<video class="bg-preview__media" src={background.src} autoplay loop muted playsinline></video>
						{:else}
							<img class="bg-preview__media" src={background.src} alt="" draggable="false" />
						{/if}
					{/key}
				{:else}
					<div class="bg-preview__empty">
						<Icon name="image" size={26} />
						<span>{t("Фон не выбран")}</span>
					</div>
				{/if}
			</div>

			<div class="bg-side">
				<div class="bg-actions">
					<button type="button" class="primary" onclick={pickBackground} disabled={background.busy}>
						<Icon name="upload" size={14} />
						{background.busy ? t("Копируем…") : background.active ? t("Заменить файл") : t("Выбрать файл")}
					</button>
					{#if background.active}
						<button type="button" class="ghost" onclick={removeBackground}>
							<Icon name="trash" size={14} /> {t("Убрать фон")}
						</button>
					{/if}
				</div>

				{#if background.info}
					<div class="bg-file">
						<Icon name={background.kind === "video" ? "play" : "image"} size={14} />
						<span class="bg-file__name">{background.info.fileName}</span>
						<span class="bg-file__size tnum">{background.sizeLabel}</span>
					</div>
				{/if}

				<div class="bg-sliders">
					<label class="bg-slider" for="bg-opacity">
						<span class="bg-slider__head">
							<span class="bg-slider__label">
								<Icon name="sun" size={13} />
								{t("Непрозрачность фона")}
							</span>
							<span class="bg-slider__value tnum">{background.opacity}%</span>
						</span>
						<input
							id="bg-opacity"
							class="bg-range"
							type="range"
							min="1"
							max="100"
							step="1"
							value={background.opacity}
							disabled={!background.active}
							style={`--fill:${background.opacity}%`}
							oninput={(e) => background.setOpacity(e.currentTarget.valueAsNumber)}
						/>
					</label>

					<label class="bg-slider" for="bg-blur">
						<span class="bg-slider__head">
							<span class="bg-slider__label">
								<Icon name="image" size={13} />
								{t("Размытие")}
							</span>
							<span class="bg-slider__value tnum">{background.blur} px</span>
						</span>
						<input
							id="bg-blur"
							class="bg-range"
							type="range"
							min="0"
							max="40"
							step="1"
							value={background.blur}
							disabled={!background.active}
							style={`--fill:${(background.blur / 40) * 100}%`}
							oninput={(e) => background.setBlur(e.currentTarget.valueAsNumber)}
						/>
					</label>
				</div>

				<div class="bg-notes">
					<p class="bg-note">{tf("PNG, JPG, GIF, WEBP — до {0} МБ · MP4, WEBM — до {1} МБ.", MAX_IMAGE_MIB, MAX_VIDEO_MIB)}</p>
					<p class="bg-note">
						{t(
							"Чем выше непрозрачность, тем ярче картинка и тем прозрачнее панели. Размытие помогает тексту оставаться читаемым поверх пёстрого фото.",
						)}
					</p>
				</div>
			</div>
		</div>
	{/if}
</div>

<style>
	.store {
		display: flex;
		flex-direction: column;
		gap: var(--sp-5);
		padding: var(--sp-6);
		overflow-y: auto;
		height: 100%;
	}

	.store__head {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		flex-wrap: wrap;
	}

	.tabs {
		display: flex;
		gap: var(--sp-1);
		padding: 4px;
		background: var(--bg-inset);
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-lg);
	}

	.tab {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		padding: 8px 14px;
		border: 0;
		border-radius: var(--r-md);
		background: transparent;
		color: var(--text-secondary);
		font-size: var(--fs-small);
		font-weight: var(--fw-medium, 500);
		cursor: pointer;
		transition: background var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out);
	}
	.tab:hover { color: var(--text-primary); background: var(--bg-hover); }
	.tab--on { background: var(--bg-raised); color: var(--text-primary); box-shadow: var(--shadow-sm); }

	.count {
		min-width: 18px;
		padding: 1px 6px;
		border-radius: var(--r-full);
		background: var(--accent-soft);
		color: var(--accent);
		font-size: var(--fs-micro);
	}

	.current {
		margin-left: auto;
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 7px 12px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-full);
		background: var(--bg-surface);
		color: var(--text-secondary);
		font-size: var(--fs-small);
	}
	.current__dot {
		width: 10px;
		height: 10px;
		border-radius: var(--r-full);
		background: var(--accent);
		box-shadow: 0 0 0 3px var(--accent-soft);
	}
	.current__text { color: var(--text-primary); }

	.notice {
		display: flex;
		align-items: center;
		gap: 9px;
		padding: 10px 14px;
		border-radius: var(--r-md);
		border: 1px solid var(--accent-border);
		background: var(--accent-soft);
		color: var(--text-primary);
		font-size: var(--fs-small);
	}
	.notice--err { border-color: var(--danger); background: var(--danger-soft); }

	.filters {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		flex-wrap: wrap;
	}
	.filters__spacer { flex: 1 1 auto; }

	.pill,
	.ghost,
	.mini,
	.primary {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		border-radius: var(--r-md);
		border: 1px solid var(--border-subtle);
		background: var(--bg-surface);
		color: var(--text-secondary);
		font-size: var(--fs-small);
		cursor: pointer;
		transition: background var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out),
			border-color var(--dur-fast) var(--ease-out);
	}
	.pill { padding: 7px 13px; }
	.ghost { padding: 8px 13px; }
	.mini { padding: 5px 10px; font-size: var(--fs-micro); }
	.primary {
		padding: 8px 15px;
		background: var(--accent);
		border-color: var(--accent);
		color: var(--accent-fg);
		font-weight: var(--fw-medium, 500);
	}
	.pill:hover,
	.ghost:hover,
	.mini:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border); }
	.primary:hover { background: var(--accent-hover); color: var(--accent-fg); }
	.pill--on {
		background: var(--accent-soft);
		border-color: var(--accent-border);
		color: var(--accent);
	}
	.mini--danger:hover { color: var(--danger); border-color: var(--danger); background: var(--danger-soft); }

	.file { display: none; }

	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(232px, 1fr));
		gap: var(--sp-4);
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: 0;
		padding: 0;
		text-align: left;
		overflow: hidden;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-lg);
		background: var(--bg-surface);
		cursor: pointer;
		transition: transform var(--dur-fast) var(--ease-out), border-color var(--dur-fast) var(--ease-out),
			box-shadow var(--dur-fast) var(--ease-out);
	}
	.card:hover { transform: translateY(-2px); border-color: var(--border-strong); box-shadow: var(--shadow-card); }
	.card--static { cursor: default; }
	.card--static:hover { transform: none; }
	.card--on { border-color: var(--accent-border); box-shadow: 0 0 0 1px var(--accent-border), var(--shadow-card); }

	.card__foot { display: flex; flex-direction: column; gap: 6px; padding: 11px 13px 13px; }
	.card__name {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--text-primary);
		font-size: var(--fs-body);
		font-weight: var(--fw-medium, 500);
	}
	.card__blurb { color: var(--text-tertiary); font-size: var(--fs-micro); line-height: 1.45; }
	.card__actions { display: flex; gap: 6px; flex-wrap: wrap; }

	.badge {
		padding: 2px 7px;
		border-radius: var(--r-full);
		background: var(--bg-inset);
		border: 1px solid var(--border-subtle);
		color: var(--text-tertiary);
		font-size: var(--fs-micro);
		font-weight: var(--fw-regular, 400);
	}
	.badge--on { background: var(--accent-soft); border-color: var(--accent-border); color: var(--accent); }

	/* ── Mini preview ── */
	.pv {
		position: relative;
		display: flex;
		gap: 6px;
		height: 118px;
		padding: 9px;
		background: var(--pv-canvas);
		border-bottom: 1px solid var(--border-subtle);
	}
	.pv__rail {
		display: flex;
		flex-direction: column;
		gap: 7px;
		width: 34%;
		padding: 8px 7px;
		border-radius: 7px;
		background: var(--pv-surface);
		border: 1px solid var(--pv-border);
	}
	.pv__dot { width: 14px; height: 14px; border-radius: 5px; background: var(--pv-accent); }
	.pv__line { height: 6px; border-radius: 3px; background: var(--pv-dim); opacity: 0.55; }
	.pv__line--short { width: 60%; }
	.pv__body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 7px;
		padding: 8px;
		border-radius: 7px;
		background: var(--pv-surface);
		border: 1px solid var(--pv-border);
	}
	.pv__bar { display: flex; align-items: center; gap: 6px; }
	.pv__title { flex: 1; height: 7px; border-radius: 4px; background: var(--pv-text); opacity: 0.8; }
	.pv__cta { width: 30px; height: 12px; border-radius: 5px; background: var(--pv-accent); }
	.pv__tiles { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; flex: 1; }
	.pv__tiles span { border-radius: 6px; background: var(--pv-raised); border: 1px solid var(--pv-border); }
	.pv__check {
		position: absolute;
		top: 8px;
		right: 8px;
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border-radius: var(--r-full);
		background: var(--accent);
		color: var(--accent-fg);
	}

	/* ── Accents ── */
	.hint { margin: 0; color: var(--text-tertiary); font-size: var(--fs-small); max-width: 70ch; line-height: 1.5; }

	.accents {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(148px, 1fr));
		gap: var(--sp-3);
	}
	.accent {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 12px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-md);
		background: var(--bg-surface);
		cursor: pointer;
		transition: border-color var(--dur-fast) var(--ease-out), background var(--dur-fast) var(--ease-out);
	}
	.accent:hover { background: var(--bg-hover); border-color: var(--border); }
	.accent--on { border-color: var(--accent-border); background: var(--accent-soft); }
	.accent__chip {
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		border-radius: 9px;
		background: var(--accent);
		color: var(--accent-fg);
		flex: none;
	}
	.accent__label { color: var(--text-primary); font-size: var(--fs-small); }

	.custom-accent {
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
		padding: var(--sp-4);
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-lg);
		background: var(--bg-surface);
	}
	.custom-accent__head { color: var(--text-primary); font-size: var(--fs-body); font-weight: var(--fw-medium, 500); }
	.custom-accent__row { display: flex; align-items: center; gap: var(--sp-2); flex-wrap: wrap; }

	.color {
		width: 46px;
		height: 34px;
		padding: 2px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-md);
		background: var(--bg-inset);
		cursor: pointer;
	}
	.text {
		width: 132px;
		padding: 8px 11px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-md);
		background: var(--bg-inset);
		color: var(--text-primary);
		font-family: var(--font-mono);
		font-size: var(--fs-small);
	}
	.text--grow { flex: 1 1 220px; width: auto; font-family: var(--font-sans); }
	.text:focus-visible { outline: none; border-color: var(--accent-border); box-shadow: 0 0 0 3px var(--accent-soft); }

	/* ── Editor ── */
	.editor {
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
		padding: var(--sp-4);
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-lg);
		background: var(--bg-surface);
	}
	.editor__row { display: flex; align-items: center; gap: var(--sp-2); flex-wrap: wrap; }
	.editor__foot { display: flex; align-items: center; gap: var(--sp-2); flex-wrap: wrap; }
	.editor__hint { color: var(--text-tertiary); font-size: var(--fs-micro); }

	.seg { display: flex; gap: 4px; padding: 4px; background: var(--bg-inset); border-radius: var(--r-md); }
	.seg__btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 11px;
		border: 0;
		border-radius: var(--r-sm);
		background: transparent;
		color: var(--text-secondary);
		font-size: var(--fs-micro);
		cursor: pointer;
	}
	.seg__btn--on { background: var(--bg-raised); color: var(--text-primary); }

	.code {
		min-height: 210px;
		padding: var(--sp-3);
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-md);
		background: var(--bg-inset);
		color: var(--text-primary);
		font-family: var(--font-mono);
		font-size: var(--fs-small);
		line-height: 1.55;
		resize: vertical;
	}
	.code:focus-visible { outline: none; border-color: var(--accent-border); box-shadow: 0 0 0 3px var(--accent-soft); }

	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 7px;
		padding: var(--sp-10) var(--sp-6);
		border: 1px dashed var(--border);
		border-radius: var(--r-lg);
		color: var(--text-tertiary);
		text-align: center;
	}
	.empty__title { color: var(--text-primary); font-size: var(--fs-body); }
	.empty__text { font-size: var(--fs-small); max-width: 46ch; }
	/* ── Fonts tab ─────────────────────────────────────────────── */

	.font-search {
		flex: 1;
		min-width: 150px;
		padding: 6px 12px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-full);
		background: var(--bg-inset);
		color: var(--text-primary);
		font-size: var(--fs-small);
	}
	.font-search:focus-visible {
		outline: none;
		border-color: var(--accent-border);
	}

	/* Size slider. Same visual language as the background sliders: a 6px
	   track whose filled part is painted from --fill, set inline. */
	.fs-row {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		flex-wrap: wrap;
	}
	.fs-step {
		flex: none;
		width: 30px;
		height: 30px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-md);
		background: var(--bg-raised);
		color: var(--text-secondary);
		font-weight: 600;
		font-size: var(--fs-small);
		cursor: pointer;
	}
	.fs-step--lg {
		font-size: var(--fs-title);
	}
	.fs-step:hover:not(:disabled) {
		border-color: var(--accent-border);
		color: var(--text-primary);
	}
	.fs-step:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.fs-range {
		--track: 6px;
		--thumb: 16px;
		flex: 1;
		min-width: 160px;
		height: var(--thumb);
		margin: 0;
		padding: 0;
		-webkit-appearance: none;
		appearance: none;
		background: transparent;
		cursor: pointer;
	}
	.fs-range::-webkit-slider-runnable-track {
		height: var(--track);
		border-radius: var(--r-full);
		background: linear-gradient(
			90deg,
			var(--accent) 0 var(--fill, 0%),
			var(--bg-active) var(--fill, 0%) 100%
		);
	}
	.fs-range::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: var(--thumb);
		height: var(--thumb);
		margin-top: calc((var(--track) - var(--thumb)) / 2);
		border: 2px solid var(--accent);
		border-radius: var(--r-full);
		background: var(--bg-raised);
	}
	.fs-range:focus-visible {
		outline: none;
	}
	.fs-range:focus-visible::-webkit-slider-thumb {
		box-shadow: 0 0 0 3px var(--accent-soft);
	}
	.fs-value {
		flex: none;
		min-width: 48px;
		padding: 2px 8px;
		border: 1px solid var(--accent-border);
		border-radius: var(--r-full);
		background: var(--accent-soft);
		color: var(--accent);
		font-size: var(--fs-micro);
		text-align: center;
	}
	.fs-preview {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--fs-body);
	}

	.font-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(186px, 1fr));
		gap: var(--sp-3);
	}

	.font-card {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 12px 13px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		color: var(--text-primary);
		text-align: left;
		cursor: pointer;
		transition:
			border-color var(--dur-fast) var(--ease-out),
			transform var(--dur-fast) var(--ease-out);
	}
	.font-card:hover {
		border-color: var(--border-strong);
		transform: translateY(-1px);
	}
	.font-card--on {
		border-color: var(--accent-border);
		background: var(--accent-soft);
	}

	.font-card__head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-2);
		color: var(--accent);
	}
	.font-card__name {
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		color: var(--text-secondary);
		font-size: var(--fs-small);
	}
	.font-card--on .font-card__name { color: var(--accent); }

	/* The sample is the whole point of the card: it must render in the family
	   the card offers, which arrives as --sample from the markup. */
	.font-card__sample {
		font-size: 21px;
		line-height: 1.25;
		color: var(--text-primary);
	}
	.font-card__sample--own { font-family: var(--sample); }
	.font-card__kind {
		color: var(--text-tertiary);
		font-size: var(--fs-micro);
	}

	.font-section {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
	}
	.font-section__title {
		color: var(--text-primary);
		font-size: var(--fs-title);
	}
	.font-chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--sp-2);
	}
	.font-chip { font-family: var(--sample); }

	.font-custom {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--sp-2);
	}
	.font-input {
		flex: 1;
		min-width: 190px;
		padding: 8px 11px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-md);
		background: var(--bg-inset);
		color: var(--text-primary);
		font-size: var(--fs-small);
	}
	.font-input:focus-visible {
		outline: none;
		border-color: var(--accent-border);
	}

	/* ── Background tab ──────────────────────────────────── */

	.bg-grid {
		display: grid;
		grid-template-columns: minmax(0, 1.3fr) minmax(258px, 1fr);
		gap: var(--sp-5);
		align-items: start;
	}

	@media (max-width: 940px) {
		.bg-grid { grid-template-columns: minmax(0, 1fr); }
	}

	.bg-preview {
		position: relative;
		aspect-ratio: 16 / 9;
		overflow: hidden;
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		background: var(--bg-inset);
	}
	.bg-preview--empty { border-style: dashed; }

	.bg-preview__media {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
		/* The preview shows the file itself, at full strength: it is a check of
		   the picture, not of the current opacity. */
		image-rendering: auto;
	}

	.bg-preview__empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		height: 100%;
		color: var(--text-tertiary);
		font-size: var(--fs-small);
	}

	.bg-side {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	.bg-actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--sp-2);
	}

	.bg-file {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-md);
		background: var(--bg-raised);
		color: var(--text-secondary);
		font-size: var(--fs-small);
	}
	.bg-file__name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		color: var(--text-primary);
	}
	.bg-file__size { color: var(--text-tertiary); }

	.bg-sliders {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
	}

	/* Deliberately not called .slider: app.css owns that class globally and
	   applies input-range geometry to anything wearing it. */
	.bg-slider {
		display: flex;
		flex-direction: column;
		gap: 9px;
		padding: 11px 13px 13px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-md);
		background: var(--bg-raised);
		transition: border-color var(--dur-fast) var(--ease-out);
	}
	.bg-slider:hover { border-color: var(--border); }
	.bg-slider:has(.bg-range:disabled) { opacity: 0.5; }

	.bg-slider__head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-2);
		font-size: var(--fs-small);
	}
	.bg-slider__label {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		color: var(--text-secondary);
	}
	.bg-slider__value {
		flex: none;
		padding: 1px 8px;
		border: 1px solid var(--accent-border);
		border-radius: var(--r-full);
		background: var(--accent-soft);
		color: var(--accent);
		font-size: var(--fs-micro);
	}

	/* The filled part of the track comes from --fill, set inline from the
	   current value: WebKit/Blink give no way to paint range progress. */
	.bg-range {
		--track: 6px;
		--thumb: 16px;
		display: block;
		width: 100%;
		height: var(--thumb);
		margin: 0;
		padding: 0;
		-webkit-appearance: none;
		appearance: none;
		background: transparent;
		cursor: pointer;
	}
	.bg-range::-webkit-slider-runnable-track {
		height: var(--track);
		border-radius: var(--r-full);
		background: linear-gradient(
			90deg,
			var(--accent) 0 var(--fill, 0%),
			var(--bg-active) var(--fill, 0%) 100%
		);
	}
	.bg-range::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: var(--thumb);
		height: var(--thumb);
		margin-top: calc((var(--track) - var(--thumb)) / 2);
		border: 2px solid var(--accent);
		border-radius: var(--r-full);
		background: var(--text-primary);
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.45);
		transition:
			box-shadow var(--dur-fast) var(--ease-out),
			transform var(--dur-fast) var(--ease-out);
	}
	.bg-range:hover::-webkit-slider-thumb {
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.45), 0 0 0 5px var(--accent-soft);
	}
	.bg-range:active::-webkit-slider-thumb {
		transform: scale(1.06);
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.45), 0 0 0 7px var(--accent-soft);
	}
	.bg-range:focus-visible { outline: none; }
	.bg-range:focus-visible::-webkit-slider-thumb {
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.45), 0 0 0 5px var(--accent-soft);
	}
	.bg-range::-moz-range-track {
		height: var(--track);
		border-radius: var(--r-full);
		background: var(--bg-active);
	}
	.bg-range::-moz-range-progress {
		height: var(--track);
		border-radius: var(--r-full);
		background: var(--accent);
	}
	.bg-range::-moz-range-thumb {
		width: var(--thumb);
		height: var(--thumb);
		border: 2px solid var(--accent);
		border-radius: var(--r-full);
		background: var(--text-primary);
	}
	.bg-range:disabled { cursor: default; }

	.bg-notes {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin-top: var(--sp-3);
		padding-top: var(--sp-4);
		border-top: 1px solid var(--border-subtle);
	}

	.bg-note {
		margin: 0;
		color: var(--text-tertiary);
		font-size: var(--fs-micro);
		line-height: var(--lh-body);
	}
</style>
