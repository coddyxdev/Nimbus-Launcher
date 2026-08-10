<script lang="ts">
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

	type Tab = "themes" | "accents" | "custom"
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

	// ── Previews ────────────────────────────────────────────────────────────

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
			say("err", "Нужен цвет в формате #RRGGBB")
			return
		}
		const normalized = value.startsWith("#") ? value : `#${value}`
		hex = normalized
		appearance.setAccent(CUSTOM_ACCENT_ID, normalized)
		say("ok", "Свой акцент применён")
	}

	function startNew(): void {
		draft = {
			id: null,
			name: "Моя тема",
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
			say("ok", apply ? "Тема сохранена и применена" : "Тема сохранена")
		} catch (err) {
			say("err", err instanceof Error ? err.message : "Не удалось сохранить тему")
		}
	}

	function removeCustom(c: CustomTheme): void {
		appearance.removeCustom(c.id)
		if (draft?.id === c.id) draft = null
		say("ok", "Тема удалена")
	}

	async function copyExport(): Promise<void> {
		if (appearance.customs.length === 0) {
			say("err", "Пока нечего экспортировать")
			return
		}
		try {
			await navigator.clipboard.writeText(appearance.exportCustoms())
			say("ok", "JSON с темами скопирован в буфер")
		} catch {
			say("err", "Буфер обмена недоступен")
		}
	}

	async function ingest(file: File): Promise<void> {
		const text = await file.text()
		if (file.name.toLowerCase().endsWith(".json")) {
			try {
				const count = appearance.importCustoms(text)
				say("ok", `Добавлено тем: ${count}`)
			} catch (err) {
				say("err", err instanceof Error ? err.message : "Файл не прочитан")
			}
			return
		}
		draft = {
			id: draft?.id ?? null,
			name: draft?.name ?? file.name.replace(/\.css$/i, ""),
			base: draft?.base ?? appearance.base,
			css: text,
		}
		say("ok", "CSS загружен в редактор")
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
		<div class="tabs" role="tablist" aria-label="Разделы оформления">
			<button type="button" role="tab" aria-selected={tab === "themes"} class="tab" class:tab--on={tab === "themes"} onclick={() => (tab = "themes")}>
				<Icon name="sparkles" size={15} /> Темы
			</button>
			<button type="button" role="tab" aria-selected={tab === "accents"} class="tab" class:tab--on={tab === "accents"} onclick={() => (tab = "accents")}>
				<Icon name="image" size={15} /> Акценты
			</button>
			<button type="button" role="tab" aria-selected={tab === "custom"} class="tab" class:tab--on={tab === "custom"} onclick={() => (tab = "custom")}>
				<Icon name="edit" size={15} /> Свои темы
				{#if appearance.customs.length > 0}<span class="count">{appearance.customs.length}</span>{/if}
			</button>
		</div>

		<div class="current">
			<span class="current__dot"></span>
			<span class="current__text">{appearance.themeName}</span>
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
			<button type="button" class="pill" class:pill--on={filter === "all"} onclick={() => (filter = "all")}>Все</button>
			<button type="button" class="pill" class:pill--on={filter === "dark"} onclick={() => (filter = "dark")}>
				<Icon name="moon" size={14} /> Тёмные
			</button>
			<button type="button" class="pill" class:pill--on={filter === "light"} onclick={() => (filter = "light")}>
				<Icon name="sun" size={14} /> Светлые
			</button>
			<span class="filters__spacer"></span>
			<button type="button" class="ghost" onclick={startNew}>
				<Icon name="plus" size={14} /> Своя тема
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
							{p.name}
							<span class="badge">
								{#if p.id === SYSTEM_ID}авто{:else if p.base === "dark"}тёмная{:else}светлая{/if}
							</span>
						</div>
						<div class="card__blurb">{p.blurb}</div>
					</div>
				</button>
			{/each}
		</div>
	{:else if tab === "accents"}
		<p class="hint">Акцент живёт отдельно от темы: любой цвет сочетается с любой темой, а оттенки наведения и свечения считаются автоматически.</p>
		<div class="accents">
			{#each ACCENTS as a (a.id)}
				<button type="button" class="accent" class:accent--on={appearance.accentId === a.id} data-accent={a.id} onclick={() => pickAccent(a.id)}>
					<span class="accent__chip">
						{#if appearance.accentId === a.id}<Icon name="check" size={14} />{/if}
					</span>
					<span class="accent__label">{a.label}</span>
				</button>
			{/each}
		</div>

		<div class="custom-accent">
			<div class="custom-accent__head">Свой цвет</div>
			<div class="custom-accent__row">
				<input class="color" type="color" aria-label="Выбрать цвет" bind:value={hex} />
				<input class="text" type="text" spellcheck="false" aria-label="HEX цвета" bind:value={hex} />
				<button type="button" class="primary" onclick={applyHex}>Применить</button>
				{#if appearance.accentId === CUSTOM_ACCENT_ID}<span class="badge badge--on">активен</span>{/if}
			</div>
		</div>
	{:else}
		<div class="filters">
			<button type="button" class="ghost" onclick={startNew}>
				<Icon name="plus" size={14} /> Новая тема
			</button>
			<button type="button" class="ghost" onclick={() => importer?.click()}>
				<Icon name="upload" size={14} /> Импорт файла
			</button>
			<button type="button" class="ghost" onclick={copyExport}>
				<Icon name="copy" size={14} /> Экспорт в буфер
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
								<span class="badge">{c.base === "dark" ? "тёмная" : "светлая"}</span>
							</div>
							<div class="card__actions">
								<button type="button" class="mini" onclick={() => pickTheme(`custom:${c.id}`)}>Применить</button>
								<button type="button" class="mini" onclick={() => startEdit(c)}>Изменить</button>
								<button type="button" class="mini mini--danger" onclick={() => removeCustom(c)}>Удалить</button>
							</div>
						</div>
					</div>
				{/each}
			</div>
		{:else if !draft}
			<div class="empty">
				<Icon name="sparkles" size={22} />
				<div class="empty__title">Своих тем пока нет</div>
				<div class="empty__text">Создайте тему на основе текущей или перетащите файл .css в редактор.</div>
			</div>
		{/if}

		{#if draft}
			<div class="editor">
				<div class="editor__row">
					<input class="text text--grow" type="text" placeholder="Название темы" aria-label="Название темы" bind:value={draft.name} />
					<div class="seg">
						<button type="button" class="seg__btn" class:seg__btn--on={draft.base === "dark"} onclick={() => draft && (draft.base = "dark")}>
							<Icon name="moon" size={14} /> Тёмная основа
						</button>
						<button type="button" class="seg__btn" class:seg__btn--on={draft.base === "light"} onclick={() => draft && (draft.base = "light")}>
							<Icon name="sun" size={14} /> Светлая основа
						</button>
					</div>
				</div>

				<textarea
					class="code"
					spellcheck="false"
					aria-label="CSS темы"
					placeholder={":root { --bg-canvas: #101015; }"}
					bind:value={draft.css}
					ondragover={(e) => e.preventDefault()}
					ondrop={onDrop}
				></textarea>

				<div class="editor__foot">
					<span class="editor__hint">Перетащите сюда файл .css — он подставится автоматически</span>
					<span class="filters__spacer"></span>
					<button type="button" class="ghost" onclick={() => (draft = null)}>Закрыть</button>
					<button type="button" class="ghost" onclick={() => saveDraft(false)}>Сохранить</button>
					<button type="button" class="primary" onclick={() => saveDraft(true)}>Сохранить и применить</button>
				</div>
			</div>
		{/if}
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
</style>
