/**
 * Appearance engine: theme presets, accent presets and user-supplied CSS.
 *
 * Design notes
 * ------------
 *  - `tokens.css` stays the single definition of the *base* dark and light
 *    palettes. A preset is therefore only a small override layer on top of a
 *    base, which keeps every preset consistent (shadows, radii, motion and
 *    every token a preset does not mention keep working).
 *  - Everything is applied by rewriting two `<style>` elements appended to
 *    `<head>`. They come after the bundled stylesheet, so plain
 *    `[data-*]` selectors win by document order without `!important`
 *    anywhere, and no component needs to know a theme exists.
 *  - The accent layer is written last so a chosen accent always overrides the
 *    accent a theme suggests.
 *  - Accent rules are emitted as `[data-accent="x"]`, not `:root[...]`, so a
 *    swatch element carrying `data-accent` previews that accent in place.
 */
import { ipc, type Theme } from "./ipc"

export type ThemeBase = "dark" | "light"

/** The subset of design tokens a preset is allowed to repaint. */
export type ThemeVars = {
	canvas: string
	surface: string
	raised: string
	hover: string
	active: string
	inset: string
	text: string
	text2: string
	text3: string
	text4: string
	skeleton: string
}

export type ThemePreset = {
	id: string
	name: string
	base: ThemeBase
	/** Accent applied together with the theme unless the user picks another. */
	accent: AccentId
	blurb: string
	/** `null` means "use the base palette from tokens.css as-is". */
	vars: ThemeVars | null
}

export type CustomTheme = {
	id: string
	name: string
	base: ThemeBase
	css: string
}

// ─── Accents ────────────────────────────────────────────────────────────────

export type AccentId = string

export type AccentKind = "solid" | "gradient"

export type AccentPreset = {
	id: AccentId
	label: string
	kind?: AccentKind
	/** Hue used on dark bases. */
	dark: string
	/** Hue used on light bases, where the same value would be too pale. */
	light: string
	/** Custom gradient definition for gradient accents. */
	gradient?: {
		dark: string
		light: string
	}
}

/**
 * Accent catalogue. Every interactive colour in the launcher resolves through
 * the `--accent*` group, so one attribute on `<html>` restyles everything.
 */
export const ACCENTS: AccentPreset[] = [
	// ── Classic & Solid Accents ──
	{ id: "jade", label: "Нефрит", kind: "solid", dark: "#3ecf8e", light: "#12a05f" },
	{ id: "emerald", label: "Изумруд", kind: "solid", dark: "#10b981", light: "#059669" },
	{ id: "mint", label: "Мята", kind: "solid", dark: "#5ad8c0", light: "#0f9184" },
	{ id: "lime", label: "Лайм", kind: "solid", dark: "#9ed164", light: "#527f17" },
	{ id: "neon-lime", label: "Неоновый лайм", kind: "solid", dark: "#84cc16", light: "#4d7c0f" },
	{ id: "teal", label: "Бирюза", kind: "solid", dark: "#14b8a6", light: "#0d9488" },
	{ id: "cyan", label: "Циан", kind: "solid", dark: "#06b6d4", light: "#0891b2" },
	{ id: "sky", label: "Небо", kind: "solid", dark: "#38bdf8", light: "#0284c7" },
	{ id: "electric", label: "Электрик", kind: "solid", dark: "#00d2ff", light: "#0284c7" },
	{ id: "azure", label: "Лазурь", kind: "solid", dark: "#4c9dfb", light: "#1668cf" },
	{ id: "sapphire", label: "Сапфир", kind: "solid", dark: "#3b82f6", light: "#1d4ed8" },
	{ id: "indigo", label: "Индиго", kind: "solid", dark: "#6366f1", light: "#4338ca" },
	{ id: "violet", label: "Аметист", kind: "solid", dark: "#8b5cf6", light: "#6d28d9" },
	{ id: "purple", label: "Пурпур", kind: "solid", dark: "#a855f7", light: "#7e22ce" },
	{ id: "lavender", label: "Лаванда", kind: "solid", dark: "#a78bfa", light: "#7c3aed" },
	{ id: "magenta", label: "Фуксия", kind: "solid", dark: "#d946ef", light: "#a21caf" },
	{ id: "plum", label: "Слива", kind: "solid", dark: "#c084fc", light: "#86198f" },
	{ id: "sakura", label: "Сакура", kind: "solid", dark: "#f472b6", light: "#db2777" },
	{ id: "rose", label: "Роза", kind: "solid", dark: "#fb7185", light: "#e11d48" },
	{ id: "crimson", label: "Багрянец", kind: "solid", dark: "#f0616b", light: "#c62f3c" },
	{ id: "ruby", label: "Рубин", kind: "solid", dark: "#e11d48", light: "#9f1239" },
	{ id: "coral", label: "Коралл", kind: "solid", dark: "#ff6b6b", light: "#d63031" },
	{ id: "ember", label: "Уголь", kind: "solid", dark: "#f97316", light: "#ea580c" },
	{ id: "sunset", label: "Мандарин", kind: "solid", dark: "#fb923c", light: "#c2410c" },
	{ id: "amber", label: "Янтарь", kind: "solid", dark: "#f59e0b", light: "#d97706" },
	{ id: "gold", label: "Золото", kind: "solid", dark: "#eab308", light: "#ca8a04" },
	{ id: "steel", label: "Сталь", kind: "solid", dark: "#94a3b8", light: "#475569" },
	{ id: "ice", label: "Лёд", kind: "solid", dark: "#a5f3fc", light: "#0e7490" },

	// ── Gradients & Special FX ──
	{
		id: "grad-cyberpunk",
		label: "Киберпанк",
		kind: "gradient",
		dark: "#00f2fe",
		light: "#0284c7",
		gradient: {
			dark: "linear-gradient(135deg, #00f2fe 0%, #4facfe 35%, #f355da 100%)",
			light: "linear-gradient(135deg, #0284c7 0%, #7c3aed 50%, #db2777 100%)",
		},
	},
	{
		id: "grad-aurora",
		label: "Северное сияние",
		kind: "gradient",
		dark: "#05ffd1",
		light: "#0d9488",
		gradient: {
			dark: "linear-gradient(135deg, #05ffd1 0%, #10b981 50%, #6366f1 100%)",
			light: "linear-gradient(135deg, #0d9488 0%, #059669 50%, #4338ca 100%)",
		},
	},
	{
		id: "grad-sunset",
		label: "Закат",
		kind: "gradient",
		dark: "#ff7e5f",
		light: "#ea580c",
		gradient: {
			dark: "linear-gradient(135deg, #ff7e5f 0%, #feb47b 45%, #ec4899 100%)",
			light: "linear-gradient(135deg, #ea580c 0%, #d97706 45%, #be185d 100%)",
		},
	},
	{
		id: "grad-cosmic",
		label: "Туманность",
		kind: "gradient",
		dark: "#a855f7",
		light: "#7e22ce",
		gradient: {
			dark: "linear-gradient(135deg, #a855f7 0%, #ec4899 50%, #38bdf8 100%)",
			light: "linear-gradient(135deg, #7e22ce 0%, #be185d 50%, #0284c7 100%)",
		},
	},
	{
		id: "grad-holographic",
		label: "Голографик",
		kind: "gradient",
		dark: "#38bdf8",
		light: "#0284c7",
		gradient: {
			dark: "linear-gradient(135deg, #ff9a9e 0%, #fecfef 25%, #a1c4fd 65%, #c2e9fb 100%)",
			light: "linear-gradient(135deg, #f43f5e 0%, #a855f7 40%, #06b6d4 100%)",
		},
	},
	{
		id: "grad-hyperpop",
		label: "Гиперпоп",
		kind: "gradient",
		dark: "#f43f5e",
		light: "#e11d48",
		gradient: {
			dark: "linear-gradient(135deg, #facc15 0%, #f43f5e 50%, #8b5cf6 100%)",
			light: "linear-gradient(135deg, #eab308 0%, #e11d48 50%, #6d28d9 100%)",
		},
	},
	{
		id: "grad-toxic",
		label: "Токсик",
		kind: "gradient",
		dark: "#a3e635",
		light: "#65a30d",
		gradient: {
			dark: "linear-gradient(135deg, #a3e635 0%, #22c55e 50%, #06b6d4 100%)",
			light: "linear-gradient(135deg, #65a30d 0%, #16a34a 50%, #0891b2 100%)",
		},
	},
	{
		id: "grad-magma",
		label: "Магма",
		kind: "gradient",
		dark: "#f97316",
		light: "#c2410c",
		gradient: {
			dark: "linear-gradient(135deg, #ef4444 0%, #f97316 50%, #eab308 100%)",
			light: "linear-gradient(135deg, #b91c1c 0%, #c2410c 50%, #a16207 100%)",
		},
	},
	{
		id: "grad-ocean",
		label: "Океан",
		kind: "gradient",
		dark: "#0ea5e9",
		light: "#0284c7",
		gradient: {
			dark: "linear-gradient(135deg, #0ea5e9 0%, #3b82f6 50%, #6366f1 100%)",
			light: "linear-gradient(135deg, #0284c7 0%, #1d4ed8 50%, #4338ca 100%)",
		},
	},
	{
		id: "grad-vaporwave",
		label: "Вейпорвейв",
		kind: "gradient",
		dark: "#c084fc",
		light: "#9333ea",
		gradient: {
			dark: "linear-gradient(135deg, #818cf8 0%, #c084fc 40%, #2dd4bf 100%)",
			light: "linear-gradient(135deg, #4f46e5 0%, #9333ea 40%, #0f766e 100%)",
		},
	},
	{
		id: "grad-matrix",
		label: "Матрица",
		kind: "gradient",
		dark: "#22c55e",
		light: "#15803d",
		gradient: {
			dark: "linear-gradient(135deg, #22c55e 0%, #4ade80 50%, #14532d 100%)",
			light: "linear-gradient(135deg, #15803d 0%, #22c55e 50%, #166534 100%)",
		},
	},
	{
		id: "grad-eclipse",
		label: "Затмение",
		kind: "gradient",
		dark: "#818cf8",
		light: "#4f46e5",
		gradient: {
			dark: "linear-gradient(135deg, #94a3b8 0%, #6366f1 50%, #a855f7 100%)",
			light: "linear-gradient(135deg, #475569 0%, #4338ca 50%, #7e22ce 100%)",
		},
	},
	{
		id: "grad-synthwave",
		label: "Синтвейв",
		kind: "gradient",
		dark: "#ec4899",
		light: "#be185d",
		gradient: {
			dark: "linear-gradient(135deg, #f43f5e 0%, #ec4899 35%, #8b5cf6 75%, #06b6d4 100%)",
			light: "linear-gradient(135deg, #e11d48 0%, #db2777 40%, #7c3aed 80%, #0891b2 100%)",
		},
	},
	{
		id: "grad-inferno",
		label: "Инферно",
		kind: "gradient",
		dark: "#ff4b1f",
		light: "#dc2626",
		gradient: {
			dark: "linear-gradient(135deg, #ff9068 0%, #ff4b1f 50%, #800909 100%)",
			light: "linear-gradient(135deg, #ea580c 0%, #dc2626 50%, #991b1b 100%)",
		},
	},
	{
		id: "grad-supernova",
		label: "Сверхновая",
		kind: "gradient",
		dark: "#00f2fe",
		light: "#0284c7",
		gradient: {
			dark: "linear-gradient(135deg, #00f2fe 0%, #9b51e0 50%, #e056fd 100%)",
			light: "linear-gradient(135deg, #0284c7 0%, #7c3aed 50%, #c026d3 100%)",
		},
	},
	{
		id: "grad-glacier",
		label: "Ледник",
		kind: "gradient",
		dark: "#67e8f9",
		light: "#0891b2",
		gradient: {
			dark: "linear-gradient(135deg, #cffafe 0%, #67e8f9 35%, #06b6d4 70%, #0284c7 100%)",
			light: "linear-gradient(135deg, #0891b2 0%, #0284c7 50%, #1e40af 100%)",
		},
	},
	{
		id: "grad-amethyst",
		label: "Аметистовый кристалл",
		kind: "gradient",
		dark: "#c084fc",
		light: "#7e22ce",
		gradient: {
			dark: "linear-gradient(135deg, #e9d5ff 0%, #c084fc 40%, #9333ea 75%, #581c87 100%)",
			light: "linear-gradient(135deg, #9333ea 0%, #7e22ce 50%, #581c87 100%)",
		},
	},
	{
		id: "grad-midnight",
		label: "Глубокая полночь",
		kind: "gradient",
		dark: "#6366f1",
		light: "#4338ca",
		gradient: {
			dark: "linear-gradient(135deg, #38bdf8 0%, #6366f1 45%, #4c1d95 100%)",
			light: "linear-gradient(135deg, #0284c7 0%, #4338ca 50%, #312e81 100%)",
		},
	},
	{
		id: "grad-emerald-glow",
		label: "Изумрудный свет",
		kind: "gradient",
		dark: "#34d399",
		light: "#059669",
		gradient: {
			dark: "linear-gradient(135deg, #a7f3d0 0%, #34d399 35%, #059669 70%, #064e3b 100%)",
			light: "linear-gradient(135deg, #059669 0%, #047857 50%, #064e3b 100%)",
		},
	},
	{
		id: "grad-sakura-bloom",
		label: "Цветение сакуры",
		kind: "gradient",
		dark: "#f472b6",
		light: "#db2777",
		gradient: {
			dark: "linear-gradient(135deg, #fbcfe8 0%, #f472b6 40%, #fb7185 75%, #e11d48 100%)",
			light: "linear-gradient(135deg, #db2777 0%, #e11d48 50%, #9f1239 100%)",
		},
	},
	{
		id: "grad-phantom",
		label: "Фантом",
		kind: "gradient",
		dark: "#2dd4bf",
		light: "#0f766e",
		gradient: {
			dark: "linear-gradient(135deg, #5eead4 0%, #2dd4bf 35%, #6366f1 75%, #312e81 100%)",
			light: "linear-gradient(135deg, #0f766e 0%, #4338ca 50%, #312e81 100%)",
		},
	},
	{
		id: "grad-blaze",
		label: "Пламя дракона",
		kind: "gradient",
		dark: "#fbbf24",
		light: "#d97706",
		gradient: {
			dark: "linear-gradient(135deg, #fef08a 0%, #fbbf24 30%, #ea580c 70%, #991b1b 100%)",
			light: "linear-gradient(135deg, #d97706 0%, #dc2626 50%, #7f1d1d 100%)",
		},
	},
]

export const CUSTOM_ACCENT_ID = "custom"

// ─── Theme presets ──────────────────────────────────────────────────────────

function preset(
	id: string,
	name: string,
	base: ThemeBase,
	accent: AccentId,
	blurb: string,
	vars: ThemeVars | null,
): ThemePreset {
	return { id, name, base, accent, blurb, vars }
}

/** Follows the operating system between the two default palettes. */
export const SYSTEM_ID = "system"

export const PRESETS: ThemePreset[] = [
	preset(SYSTEM_ID, "Системная", "dark", "jade", "Следует за оформлением Windows", null),

	// ── Dark ────────────────────────────────────────────────────────────────
	preset("obsidian", "Обсидиан", "dark", "jade", "Нейтральный тёмный по умолчанию", null),
	preset("carbon", "Карбон", "dark", "mint", "Чистый чёрный для OLED", {
		canvas: "#000000",
		surface: "#08080a",
		raised: "#101012",
		hover: "#17171a",
		active: "#1f1f23",
		inset: "#000000",
		text: "#f2f2f4",
		text2: "#a2a2aa",
		text3: "#6e6e78",
		text4: "#43434b",
		skeleton: "#131316",
	}),
	preset("midnight", "Полночь", "dark", "azure", "Глубокий синий с холодным светом", {
		canvas: "#080b16",
		surface: "#0d111f",
		raised: "#141a2b",
		hover: "#1b2236",
		active: "#232c44",
		inset: "#050710",
		text: "#eef2fb",
		text2: "#a3adc6",
		text3: "#6f7892",
		text4: "#464e63",
		skeleton: "#171e30",
	}),
	preset("nord", "Норд", "dark", "sky", "Скандинавский полярный набор", {
		canvas: "#242933",
		surface: "#2e3440",
		raised: "#3b4252",
		hover: "#434c5e",
		active: "#4c566a",
		inset: "#1f242e",
		text: "#eceff4",
		text2: "#c3ccd9",
		text3: "#8c97a8",
		text4: "#5c6675",
		skeleton: "#39404e",
	}),
	preset("dracula", "Дракула", "dark", "magenta", "Контрастная классика с фиолетовым", {
		canvas: "#1e1f29",
		surface: "#282a36",
		raised: "#333546",
		hover: "#3d4055",
		active: "#464a63",
		inset: "#191a22",
		text: "#f8f8f2",
		text2: "#c3c3d4",
		text3: "#8b8ca3",
		text4: "#5c5d73",
		skeleton: "#313343",
	}),
	preset("tokyo", "Токио", "dark", "indigo", "Ночной неон, мягкие синие тона", {
		canvas: "#16161e",
		surface: "#1a1b26",
		raised: "#222333",
		hover: "#2a2b3d",
		active: "#333549",
		inset: "#101018",
		text: "#c8d0f0",
		text2: "#9aa2c7",
		text3: "#6b7297",
		text4: "#464b66",
		skeleton: "#212233",
	}),
	preset("mocha", "Мокко", "dark", "violet", "Тёплый пастельный тёмный", {
		canvas: "#181825",
		surface: "#1e1e2e",
		raised: "#282839",
		hover: "#313244",
		active: "#3b3c52",
		inset: "#11111b",
		text: "#cdd6f4",
		text2: "#a6adc8",
		text3: "#7f849c",
		text4: "#585b70",
		skeleton: "#272838",
	}),
	preset("gruvbox", "Грувбокс", "dark", "amber", "Ретро-палитра с тёплой землёй", {
		canvas: "#1d2021",
		surface: "#282828",
		raised: "#32302f",
		hover: "#3c3836",
		active: "#504945",
		inset: "#171819",
		text: "#fbf1c7",
		text2: "#d5c4a1",
		text3: "#a89984",
		text4: "#7c6f64",
		skeleton: "#332f2d",
	}),
	preset("everforest", "Лес", "dark", "lime", "Приглушённая зелень и мягкий контраст", {
		canvas: "#272e33",
		surface: "#2d353b",
		raised: "#374145",
		hover: "#414b50",
		active: "#4c555b",
		inset: "#20272b",
		text: "#d3c6aa",
		text2: "#a7b8a4",
		text3: "#859289",
		text4: "#5c6a72",
		skeleton: "#343f44",
	}),
	preset("rosepine", "Розовая сосна", "dark", "rose", "Тёмно-сливовый с розовым акцентом", {
		canvas: "#191724",
		surface: "#1f1d2e",
		raised: "#26233a",
		hover: "#2f2b45",
		active: "#393552",
		inset: "#14121f",
		text: "#e0def4",
		text2: "#b6b3d0",
		text3: "#8880a6",
		text4: "#5b567a",
		skeleton: "#252239",
	}),
	preset("abyss", "Бездна", "dark", "mint", "Морская глубина, холодный бирюзовый", {
		canvas: "#061417",
		surface: "#0a1c21",
		raised: "#0f272e",
		hover: "#15323b",
		active: "#1c3f4a",
		inset: "#040f11",
		text: "#e2f2f4",
		text2: "#9fb9be",
		text3: "#6c8a90",
		text4: "#44585d",
		skeleton: "#122a31",
	}),
	preset("mono", "Графит", "dark", "steel", "Строгий серый без единого оттенка", {
		canvas: "#121212",
		surface: "#181818",
		raised: "#1f1f1f",
		hover: "#272727",
		active: "#303030",
		inset: "#0d0d0d",
		text: "#ededed",
		text2: "#a8a8a8",
		text3: "#787878",
		text4: "#4d4d4d",
		skeleton: "#212121",
	}),

	// ── Light ───────────────────────────────────────────────────────────────
	preset("paper", "Бумага", "light", "jade", "Светлый по умолчанию", null),
	preset("latte", "Латте", "light", "violet", "Мягкий тёплый светлый", {
		canvas: "#eff1f5",
		surface: "#f6f7fa",
		raised: "#ffffff",
		hover: "#e6e9ef",
		active: "#dce0e8",
		inset: "#e9ecf2",
		text: "#4c4f69",
		text2: "#5c5f77",
		text3: "#8c8fa1",
		text4: "#acb0be",
		skeleton: "#e2e5ec",
	}),
	preset("solarized", "Соляр", "light", "amber", "Кремовая бумага, низкий контраст", {
		canvas: "#f4ecd8",
		surface: "#fdf6e3",
		raised: "#fffaf0",
		hover: "#efe7d2",
		active: "#e6ddc6",
		inset: "#eee6d2",
		text: "#3f4b5f",
		text2: "#5f6f72",
		text3: "#8a9698",
		text4: "#b0b8b6",
		skeleton: "#e9e1cd",
	}),
	preset("frost", "Иней", "light", "azure", "Холодный светлый с синевой", {
		canvas: "#eef2f7",
		surface: "#f7fafd",
		raised: "#ffffff",
		hover: "#e5ebf3",
		active: "#d9e1ec",
		inset: "#e8eef6",
		text: "#16202e",
		text2: "#4a5768",
		text3: "#78879a",
		text4: "#a8b4c2",
		skeleton: "#e2e8f1",
	}),
]

// ─── Storage keys ───────────────────────────────────────────────────────────

const THEME_KEY = "nimbus.theme.preset"
const ACCENT_KEY = "nimbus.accent"
const ACCENT_HEX_KEY = "nimbus.accent.custom"
const CUSTOM_KEY = "nimbus.themes.custom"

const DEFAULT_ACCENT = "jade"
const DEFAULT_ACCENT_HEX = "#3ecf8e"

// ─── Colour helpers ─────────────────────────────────────────────────────────

type Rgb = { r: number; g: number; b: number }

function parseHex(hex: string): Rgb | null {
	const value = hex.trim().replace(/^#/, "")
	const full =
		value.length === 3
			? value
					.split("")
					.map((c) => c + c)
					.join("")
			: value
	if (!/^[0-9a-fA-F]{6}$/.test(full)) return null
	return {
		r: parseInt(full.slice(0, 2), 16),
		g: parseInt(full.slice(2, 4), 16),
		b: parseInt(full.slice(4, 6), 16),
	}
}

function toHex(c: Rgb): string {
	const part = (n: number) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0")
	return `#${part(c.r)}${part(c.g)}${part(c.b)}`
}

/** Mixes towards white (amount > 0) or black (amount < 0). */
function shade(hex: string, amount: number): string {
	const c = parseHex(hex)
	if (!c) return hex
	const target = amount >= 0 ? 255 : 0
	const k = Math.abs(amount)
	return toHex({
		r: c.r + (target - c.r) * k,
		g: c.g + (target - c.g) * k,
		b: c.b + (target - c.b) * k,
	})
}

function rgba(hex: string, alpha: number): string {
	const c = parseHex(hex)
	if (!c) return hex
	return `rgba(${c.r}, ${c.g}, ${c.b}, ${alpha})`
}

/** Perceived luminance, used to pick readable text on top of the accent. */
function luminance(hex: string): number {
	const c = parseHex(hex)
	if (!c) return 0
	return (0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b) / 255
}

function accentBlock(selector: string, a: AccentPreset, base: ThemeBase): string {
	const hex = base === "light" ? a.light : a.dark
	const fg = luminance(hex) > 0.55 ? shade(hex, -0.86) : "#ffffff"
	const grad = a.gradient
		? (base === "light" ? a.gradient.light : a.gradient.dark)
		: `linear-gradient(135deg,${shade(hex, 0.12)} 0%,${hex} 50%,${shade(hex, -0.1)} 100%)`
	return [
		`${selector}{`,
		`--accent:${hex};`,
		`--accent-hover:${shade(hex, 0.1)};`,
		`--accent-pressed:${shade(hex, -0.12)};`,
		`--accent-fg:${fg};`,
		`--accent-soft:${rgba(hex, 0.15)};`,
		`--accent-border:${rgba(hex, 0.45)};`,
		`--accent-glow:${rgba(hex, 0.35)};`,
		`--gradient-accent:${grad};`,
		`--gradient-gold-soft:linear-gradient(180deg,${rgba(hex, 0.12)},${rgba(hex, 0.03)});`,
		"}",
	].join("")
}

function accentCustomBlock(selector: string, hex: string): string {
	const fg = luminance(hex) > 0.55 ? shade(hex, -0.86) : "#ffffff"
	return [
		`${selector}{`,
		`--accent:${hex};`,
		`--accent-hover:${shade(hex, 0.1)};`,
		`--accent-pressed:${shade(hex, -0.12)};`,
		`--accent-fg:${fg};`,
		`--accent-soft:${rgba(hex, 0.13)};`,
		`--accent-border:${rgba(hex, 0.4)};`,
		`--accent-glow:${rgba(hex, 0.3)};`,
		`--gradient-accent:linear-gradient(135deg,${shade(hex, 0.12)} 0%,${hex} 50%,${shade(hex, -0.1)} 100%);`,
		`--gradient-gold-soft:linear-gradient(180deg,${rgba(hex, 0.1)},${rgba(hex, 0.03)});`,
		"}",
	].join("")
}

// ─── CSS emitters ───────────────────────────────────────────────────────────

function varsBlock(vars: ThemeVars): string {
	return [
		":root{",
		`--bg-canvas:${vars.canvas};`,
		`--bg-surface:${vars.surface};`,
		`--bg-raised:${vars.raised};`,
		`--bg-hover:${vars.hover};`,
		`--bg-active:${vars.active};`,
		`--bg-inset:${vars.inset};`,
		`--text-primary:${vars.text};`,
		`--text-secondary:${vars.text2};`,
		`--text-tertiary:${vars.text3};`,
		`--text-disabled:${vars.text4};`,
		`--skeleton-base:${vars.skeleton};`,
		"}",
	].join("")
}

/** The CSS a user gets when duplicating a preset into their own theme. */
export function presetToCss(p: ThemePreset): string {
	const vars = p.vars ?? BASE_VARS[p.base]
	return [
		":root {",
		`\t--bg-canvas: ${vars.canvas};`,
		`\t--bg-surface: ${vars.surface};`,
		`\t--bg-raised: ${vars.raised};`,
		`\t--bg-hover: ${vars.hover};`,
		`\t--bg-active: ${vars.active};`,
		`\t--bg-inset: ${vars.inset};`,
		`\t--text-primary: ${vars.text};`,
		`\t--text-secondary: ${vars.text2};`,
		`\t--text-tertiary: ${vars.text3};`,
		`\t--text-disabled: ${vars.text4};`,
		`\t--skeleton-base: ${vars.skeleton};`,
		"}",
	].join("\n")
}

/** Mirrors the two base palettes from tokens.css for previews and templates. */
export const BASE_VARS: Record<ThemeBase, ThemeVars> = {
	dark: {
		canvas: "#0a0a0c",
		surface: "#101013",
		raised: "#17171b",
		hover: "#1e1e23",
		active: "#26262c",
		inset: "#070709",
		text: "#f4f4f6",
		text2: "#a5a5ae",
		text3: "#72727c",
		text4: "#47474f",
		skeleton: "#1a1a1e",
	},
	light: {
		canvas: "#f6f6f7",
		surface: "#fbfbfc",
		raised: "#ffffff",
		hover: "#f1f1f3",
		active: "#e7e7ea",
		inset: "#f2f2f4",
		text: "#16161a",
		text2: "#55555f",
		text3: "#82828d",
		text4: "#b4b4bc",
		skeleton: "#ebebee",
	},
}

/**
 * Strips the handful of CSS constructs that have no place in a colour theme
 * and could pull in remote content or break out of the style element.
 */
export function sanitizeCss(css: string): string {
	let out = css.replace(/<\/?\s*style/gi, "")
	out = out.replace(/@import[^;]*;?/gi, "")
	out = out.replace(/@charset[^;]*;?/gi, "")
	out = out.replace(/expression\s*\(/gi, "(")
	out = out.replace(/javascript:/gi, "")
	out = out.replace(/url\s*\(\s*['"]?\s*(?!data:image\/)[a-z]+:/gi, "url(")
	return out.trim()
}

/** Accepts either a full rule set or a bare list of declarations. */
function wrapCss(css: string): string {
	const clean = sanitizeCss(css)
	if (!clean) return ""
	return clean.includes("{") ? clean : `:root{${clean}}`
}

/** Pulls known tokens out of arbitrary CSS so a custom theme can be previewed. */
export function readCssVar(css: string, name: string): string | null {
	const match = new RegExp(`--${name}\\s*:\\s*([^;}\\n]+)`).exec(css)
	return match?.[1]?.trim() ?? null
}

// ─── DOM plumbing ───────────────────────────────────────────────────────────

function styleEl(id: string): HTMLStyleElement {
	const existing = document.getElementById(id)
	if (existing instanceof HTMLStyleElement) return existing
	const el = document.createElement("style")
	el.id = id
	document.head.appendChild(el)
	return el
}

const media =
	typeof window !== "undefined" && typeof window.matchMedia === "function"
		? window.matchMedia("(prefers-color-scheme: light)")
		: null

function systemBase(): ThemeBase {
	return media?.matches ? "light" : "dark"
}

function read(key: string): string | null {
	try {
		return window.localStorage.getItem(key)
	} catch {
		return null
	}
}

function write(key: string, value: string): void {
	try {
		window.localStorage.setItem(key, value)
	} catch {
		// Private mode: the preference simply does not survive a restart.
	}
}

function readCustoms(): CustomTheme[] {
	const raw = read(CUSTOM_KEY)
	if (!raw) return []
	try {
		const parsed: unknown = JSON.parse(raw)
		if (!Array.isArray(parsed)) return []
		return parsed.filter(isCustomTheme)
	} catch {
		return []
	}
}

function isCustomTheme(value: unknown): value is CustomTheme {
	if (typeof value !== "object" || value === null) return false
	const v = value as Record<string, unknown>
	return (
		typeof v.id === "string" &&
		typeof v.name === "string" &&
		typeof v.css === "string" &&
		(v.base === "dark" || v.base === "light")
	)
}

// ─── Store ──────────────────────────────────────────────────────────────────

class Appearance {
	/** Preset id, `system`, or `custom:<id>`. */
	themeId = $state<string>(read(THEME_KEY) ?? SYSTEM_ID)
	accentId = $state<AccentId>(read(ACCENT_KEY) ?? DEFAULT_ACCENT)
	accentHex = $state<string>(read(ACCENT_HEX_KEY) ?? DEFAULT_ACCENT_HEX)
	customs = $state<CustomTheme[]>(readCustoms())

	/** The built-in preset behind the current selection, if it is one. */
	get preset(): ThemePreset | null {
		return PRESETS.find((p) => p.id === this.themeId) ?? null
	}

	get custom(): CustomTheme | null {
		return this.customs.find((c) => `custom:${c.id}` === this.themeId) ?? null
	}

	get base(): ThemeBase {
		const custom = this.custom
		if (custom) return custom.base
		const preset = this.preset
		if (!preset || preset.id === SYSTEM_ID) return systemBase()
		return preset.base
	}

	get themeName(): string {
		return this.custom?.name ?? this.preset?.name ?? "Обсидиан"
	}

	/** Resolved accent colour for the current base, used by previews. */
	get accentHexResolved(): string {
		if (this.accentId === CUSTOM_ACCENT_ID) return this.accentHex
		const found = ACCENTS.find((a) => a.id === this.accentId)
		if (!found) return DEFAULT_ACCENT_HEX
		return this.base === "light" ? found.light : found.dark
	}

	/** Repaints `<html>` and both style elements from the current state. */
	apply(): void {
		if (typeof document === "undefined") return
		const base = this.base
		const root = document.documentElement
		root.dataset.theme = base
		root.dataset.themeId = this.themeId
		root.dataset.accent = this.accentId

		const custom = this.custom
		const preset = this.preset
		const themeCss = custom
			? wrapCss(custom.css)
			: preset?.vars
				? varsBlock(preset.vars)
				: ""
		styleEl("nimbus-theme").textContent = themeCss

		// Written after the theme layer and re-appended so it always wins.
		const accents = styleEl("nimbus-accent")
		const blocks = ACCENTS.map((a) =>
			[
				accentBlock(`[data-accent="${a.id}"]`, a, "dark"),
				accentBlock(`[data-theme="light"][data-accent="${a.id}"]`, a, "light"),
			].join(""),
		)
		blocks.push(accentCustomBlock(`[data-accent="${CUSTOM_ACCENT_ID}"]`, this.accentHex))
		accents.textContent = blocks.join("")
		document.head.appendChild(accents)
	}

	/** Applies a theme and its suggested accent. */
	setTheme(id: string, opts: { syncBackend?: boolean } = {}): void {
		this.themeId = id
		write(THEME_KEY, id)
		const preset = this.preset
		if (preset && preset.id !== SYSTEM_ID) {
			this.accentId = preset.accent
			write(ACCENT_KEY, preset.accent)
		}
		this.apply()
		if (opts.syncBackend !== false) this.syncBackend()
	}

	setAccent(id: AccentId, hex?: string): void {
		this.accentId = id
		write(ACCENT_KEY, id)
		if (id === CUSTOM_ACCENT_ID && hex) {
			this.accentHex = hex
			write(ACCENT_HEX_KEY, hex)
		}
		this.apply()
	}

	/** Keeps the Rust config in step so the next boot starts on the same base. */
	syncBackend(): void {
		const theme: Theme = this.themeId === SYSTEM_ID ? "system" : this.base
		void ipc.setTheme(theme).catch(() => {
			// Non-critical: the theme is already applied client-side.
		})
	}

	/** Boot path: restores the stored theme, falling back to the saved config. */
	hydrate(configTheme: Theme): void {
		const stored = read(THEME_KEY)
		const known =
			stored !== null &&
			(PRESETS.some((p) => p.id === stored) ||
				this.customs.some((c) => `custom:${c.id}` === stored))
		this.themeId = known && stored !== null ? stored : baseThemeId(configTheme)
		this.apply()
	}

	/** Explicit dark / light / system switch coming from Settings. */
	selectBase(theme: Theme): void {
		this.setTheme(baseThemeId(theme))
	}

	// ── Custom themes ─────────────────────────────────────────────────────

	saveCustom(input: { id?: string; name: string; base: ThemeBase; css: string }): CustomTheme {
		const id = input.id ?? `t${Date.now().toString(36)}`
		const entry: CustomTheme = {
			id,
			name: input.name.trim() || "Моя тема",
			base: input.base,
			css: sanitizeCss(input.css),
		}
		const next = this.customs.filter((c) => c.id !== id)
		next.push(entry)
		this.customs = next
		this.persistCustoms()
		if (this.themeId === `custom:${id}`) this.apply()
		return entry
	}

	removeCustom(id: string): void {
		this.customs = this.customs.filter((c) => c.id !== id)
		this.persistCustoms()
		if (this.themeId === `custom:${id}`) this.setTheme(SYSTEM_ID)
	}

	private persistCustoms(): void {
		write(CUSTOM_KEY, JSON.stringify(this.customs))
	}

	/** Everything the user made, as a portable JSON document. */
	exportCustoms(): string {
		return JSON.stringify({ nimbusThemes: 1, themes: this.customs }, null, 2)
	}

	/** Accepts either the export format or a bare array of themes. */
	importCustoms(json: string): number {
		const parsed: unknown = JSON.parse(json)
		const list: unknown = Array.isArray(parsed)
			? parsed
			: (parsed as { themes?: unknown })?.themes
		if (!Array.isArray(list)) throw new Error("Ожидался список тем")
		const incoming = list.filter(isCustomTheme)
		if (incoming.length === 0) throw new Error("В файле нет ни одной темы")
		for (const theme of incoming) {
			this.saveCustom({
				id: this.customs.some((c) => c.id === theme.id) ? undefined : theme.id,
				name: theme.name,
				base: theme.base,
				css: theme.css,
			})
		}
		return incoming.length
	}
}

/** `dark` / `light` / `system` mapped onto the matching default preset. */
function baseThemeId(theme: Theme): string {
	if (theme === "light") return "paper"
	if (theme === "dark") return "obsidian"
	return SYSTEM_ID
}

export const appearance = new Appearance()

/** Re-applies on OS change while the system theme is selected. */
export function watchSystemAppearance(): () => void {
	if (!media) return () => {}
	const handler = () => {
		if (appearance.themeId === SYSTEM_ID) appearance.apply()
	}
	media.addEventListener("change", handler)
	return () => media.removeEventListener("change", handler)
}
