/**
 * Font picker for the appearance section.
 *
 * The launcher is offline-first, so nothing here downloads a webfont: the
 * catalogue lists families that ship with Windows (plus a few common
 * developer/mac/linux ones) and every entry is probed against the machine
 * before it is offered. A font the user cannot actually render is worse than
 * no font at all, hence `isInstalled`.
 *
 * Applying writes the three type tokens on <html>, which is all the rest of
 * the UI reads, so a change repaints the whole launcher instantly.
 */

export type FontKind = "sans" | "serif" | "mono" | "display"

export type FontDef = {
	id: string
	family: string
	kind: FontKind
}

/** Generic CSS fallback appended after every choice. */
const FALLBACK: Record<FontKind, string> = {
	sans: "system-ui, sans-serif",
	serif: "Georgia, serif",
	mono: "ui-monospace, Consolas, monospace",
	display: "system-ui, sans-serif",
}

function def(family: string, kind: FontKind): FontDef {
	return { id: family.toLowerCase().replace(/[^a-z0-9]+/g, "-"), family, kind }
}

/**
 * The catalogue. Deliberately long: most of these ship with Windows, and the
 * availability probe quietly drops whatever is missing on a given machine.
 */
export const FONTS: FontDef[] = [
	// ── Sans ────────────────────────────────────────────────────────────
	def("Segoe UI Variable Text", "sans"),
	def("Segoe UI", "sans"),
	def("Segoe UI Semibold", "sans"),
	def("Bahnschrift", "sans"),
	def("Calibri", "sans"),
	def("Candara", "sans"),
	def("Corbel", "sans"),
	def("Tahoma", "sans"),
	def("Verdana", "sans"),
	def("Trebuchet MS", "sans"),
	def("Arial", "sans"),
	def("Arial Narrow", "sans"),
	def("Arial Black", "sans"),
	def("Franklin Gothic Book", "sans"),
	def("Franklin Gothic Medium", "sans"),
	def("Century Gothic", "sans"),
	def("Gill Sans MT", "sans"),
	def("Lucida Sans Unicode", "sans"),
	def("Microsoft Sans Serif", "sans"),
	def("MS Reference Sans Serif", "sans"),
	def("Ebrima", "sans"),
	def("Nirmala UI", "sans"),
	def("Leelawadee UI", "sans"),
	def("Malgun Gothic", "sans"),
	def("Yu Gothic UI", "sans"),
	def("Tw Cen MT", "sans"),
	def("Berlin Sans FB", "sans"),
	def("Maiandra GD", "sans"),
	def("Eras Medium ITC", "sans"),
	def("Inter", "sans"),
	def("Roboto", "sans"),
	def("Open Sans", "sans"),
	def("Noto Sans", "sans"),
	def("Source Sans Pro", "sans"),
	def("Helvetica Neue", "sans"),
	def("Ubuntu", "sans"),
	def("Cantarell", "sans"),
	def("DejaVu Sans", "sans"),
	def("Liberation Sans", "sans"),

	// ── Serif ───────────────────────────────────────────────────────────
	def("Georgia", "serif"),
	def("Cambria", "serif"),
	def("Constantia", "serif"),
	def("Times New Roman", "serif"),
	def("Palatino Linotype", "serif"),
	def("Book Antiqua", "serif"),
	def("Bookman Old Style", "serif"),
	def("Sitka Text", "serif"),
	def("Sitka Heading", "serif"),
	def("Sylfaen", "serif"),
	def("Garamond", "serif"),
	def("Baskerville Old Face", "serif"),
	def("Bodoni MT", "serif"),
	def("Californian FB", "serif"),
	def("Centaur", "serif"),
	def("Century Schoolbook", "serif"),
	def("Goudy Old Style", "serif"),
	def("High Tower Text", "serif"),
	def("Lucida Bright", "serif"),
	def("Lucida Fax", "serif"),
	def("Perpetua", "serif"),
	def("Rockwell", "serif"),
	def("Footlight MT Light", "serif"),
	def("Modern No. 20", "serif"),
	def("Poor Richard", "serif"),
	def("Cooper Black", "serif"),
	def("Elephant", "serif"),
	def("Noto Serif", "serif"),
	def("DejaVu Serif", "serif"),

	// ── Monospace ───────────────────────────────────────────────────────
	def("Cascadia Code", "mono"),
	def("Cascadia Mono", "mono"),
	def("Consolas", "mono"),
	def("Courier New", "mono"),
	def("Lucida Console", "mono"),
	def("Lucida Sans Typewriter", "mono"),
	def("OCR A Extended", "mono"),
	def("JetBrains Mono", "mono"),
	def("Fira Code", "mono"),
	def("Source Code Pro", "mono"),
	def("IBM Plex Mono", "mono"),
	def("Roboto Mono", "mono"),
	def("Ubuntu Mono", "mono"),
	def("DejaVu Sans Mono", "mono"),
	def("Liberation Mono", "mono"),
	def("Menlo", "mono"),
	def("Monaco", "mono"),
	def("SF Mono", "mono"),

	// ── Display / handwriting ───────────────────────────────────────────
	def("Impact", "display"),
	def("Haettenschweiler", "display"),
	def("Bernard MT Condensed", "display"),
	def("Britannic Bold", "display"),
	def("Bauhaus 93", "display"),
	def("Broadway", "display"),
	def("Castellar", "display"),
	def("Colonna MT", "display"),
	def("Copperplate Gothic Bold", "display"),
	def("Engravers MT", "display"),
	def("Felix Titling", "display"),
	def("Forte", "display"),
	def("Goudy Stout", "display"),
	def("Harrington", "display"),
	def("Imprint MT Shadow", "display"),
	def("Magneto", "display"),
	def("Niagara Solid", "display"),
	def("Onyx", "display"),
	def("Playbill", "display"),
	def("Ravie", "display"),
	def("Showcard Gothic", "display"),
	def("Snap ITC", "display"),
	def("Stencil", "display"),
	def("Algerian", "display"),
	def("Wide Latin", "display"),
	def("Comic Sans MS", "display"),
	def("Segoe Print", "display"),
	def("Segoe Script", "display"),
	def("Ink Free", "display"),
	def("MV Boli", "display"),
	def("Gabriola", "display"),
	def("Papyrus", "display"),
	def("Brush Script MT", "display"),
	def("Lucida Handwriting", "display"),
	def("Lucida Calligraphy", "display"),
	def("Monotype Corsiva", "display"),
	def("Freestyle Script", "display"),
	def("French Script MT", "display"),
	def("Edwardian Script ITC", "display"),
	def("Kunstler Script", "display"),
	def("Script MT Bold", "display"),
	def("Mistral", "display"),
	def("Bradley Hand ITC", "display"),
	def("Kristen ITC", "display"),
	def("Juice ITC", "display"),
	def("Curlz MT", "display"),
	def("Chiller", "display"),
	def("Jokerman", "display"),
	def("Informal Roman", "display"),
	def("Matura MT Script Capitals", "display"),
	def("Old English Text MT", "display"),
	def("Blackadder ITC", "display"),
	def("Parchment", "display"),
	def("Pristina", "display"),
	def("Rage Italic", "display"),
	def("Tempus Sans ITC", "display"),
	def("Viner Hand ITC", "display"),
	def("Vivaldi", "display"),
	def("Gigi", "display"),
]

/** Quotes a family name so it survives inside a CSS font-family list. */
function quoted(family: string): string {
	return `"${family.replace(/["\\]/g, "")}"`
}

/** Full CSS font-family value for a catalogue entry. */
export function stackOf(font: FontDef): string {
	return `${quoted(font.family)}, ${FALLBACK[font.kind]}`
}

// ── Availability probing ───────────────────────────────────────────────

// Latin only: a family without Cyrillic would borrow those glyphs from the
// fallback and blur the measurement.
const PROBE = "mmmmmmmmmmlliWWQQ0OA"
const BASELINES = ["monospace", "serif", "sans-serif"] as const

const probed = new Map<string, boolean>()
let measurer: CanvasRenderingContext2D | null | undefined

function context(): CanvasRenderingContext2D | null {
	if (measurer === undefined) {
		measurer = document.createElement("canvas").getContext("2d")
	}
	return measurer
}

function widthOf(ctx: CanvasRenderingContext2D, font: string): number {
	ctx.font = font
	return ctx.measureText(PROBE).width
}

/**
 * True when the family is actually present on this machine.
 *
 * Rendering the probe against each generic baseline and comparing widths is
 * the only reliable check: `document.fonts.check` answers for webfonts, not
 * for locally installed ones. Three baselines are needed because a font that
 * *is* the platform default for one generic (Arial for sans-serif on Windows)
 * would otherwise look missing.
 */
export function isInstalled(family: string): boolean {
	if (typeof document === "undefined") return false
	const cached = probed.get(family)
	if (cached !== undefined) return cached

	const ctx = context()
	if (!ctx) return false

	let found = false
	for (const baseline of BASELINES) {
		const plain = widthOf(ctx, `72px ${baseline}`)
		const candidate = widthOf(ctx, `72px ${quoted(family)}, ${baseline}`)
		if (Math.abs(candidate - plain) > 0.5) {
			found = true
			break
		}
	}
	probed.set(family, found)
	return found
}

// ── State ──────────────────────────────────────────────────────────────

const STORAGE_KEY = "nimbus.fonts"

/** Sentinel meaning "leave the design tokens alone". */
export const DEFAULT_ID = "default"

/**
 * Family every profile starts on, and the one "restore defaults" returns to.
 * A missing Arial Black simply falls through to the generic stack, so this is
 * safe on machines that do not ship it.
 */
export const DEFAULT_UI_FAMILY = "Arial Black"

/**
 * Base type scale copied from tokens.css. The slider rescales every one of
 * these together, so headings and captions keep their relative proportions
 * instead of drifting apart.
 */
const BASE_SIZES: Record<string, number> = {
	"--fs-micro": 11,
	"--fs-small": 12,
	"--fs-body": 13,
	"--fs-title": 15,
	"--fs-display": 20,
	"--fs-hero": 28,
}

/** Type scale in percent. 100 means "exactly what the design specifies". */
export const DEFAULT_SCALE = 100
export const MIN_SCALE = 80
export const MAX_SCALE = 140

/**
 * Keeps the scale inside the supported range and on a whole step. A value
 * from corrupt storage must never be able to make the UI unreadable.
 */
function clampScale(percent: number): number {
	if (!Number.isFinite(percent)) return DEFAULT_SCALE
	const stepped = Math.round(percent / 5) * 5
	return Math.min(MAX_SCALE, Math.max(MIN_SCALE, stepped))
}

type Saved = { ui?: string; mono?: string; scale?: number }

function readStored(): Saved {
	try {
		const raw = localStorage.getItem(STORAGE_KEY)
		if (!raw) return {}
		const parsed: unknown = JSON.parse(raw)
		if (!parsed || typeof parsed !== "object") return {}
		const { ui, mono, scale } = parsed as Saved
		return {
			ui: typeof ui === "string" ? ui : undefined,
			mono: typeof mono === "string" ? mono : undefined,
			scale: typeof scale === "number" ? clampScale(scale) : undefined,
		}
	} catch {
		// Corrupt or unavailable storage must never block the UI.
		return {}
	}
}

/**
 * A saved family is kept as a plain name rather than a catalogue id: a custom
 * family typed by the user is not in the catalogue, and a catalogue entry
 * renamed later should not silently resolve to something else.
 */
class FontState {
	ui = $state<string>(DEFAULT_UI_FAMILY)
	mono = $state<string>(DEFAULT_ID)
	scale = $state<number>(DEFAULT_SCALE)

	/** Reads storage and paints the tokens. Call once, on boot. */
	hydrate(): void {
		const saved = readStored()
		this.ui = saved.ui ?? DEFAULT_UI_FAMILY
		this.mono = saved.mono ?? DEFAULT_ID
		this.scale = clampScale(saved.scale ?? DEFAULT_SCALE)
		this.paint()
	}

	setUi(family: string): void {
		this.ui = family
		this.persist()
		this.paint()
	}

	setMono(family: string): void {
		this.mono = family
		this.persist()
		this.paint()
	}

	/** Sets the type scale in percent. Out-of-range values are clamped. */
	setScale(percent: number): void {
		this.scale = clampScale(percent)
		this.persist()
		this.paint()
	}

	reset(): void {
		this.ui = DEFAULT_UI_FAMILY
		this.mono = DEFAULT_ID
		this.scale = DEFAULT_SCALE
		this.persist()
		this.paint()
	}

	private persist(): void {
		try {
			localStorage.setItem(
				STORAGE_KEY,
				JSON.stringify({ ui: this.ui, mono: this.mono, scale: this.scale }),
			)
		} catch {
			// Session-only choice is still better than refusing to switch.
		}
	}

	/** Writes the type tokens on <html>, or clears them for the default. */
	private paint(): void {
		if (typeof document === "undefined") return
		const root = document.documentElement.style

		if (this.ui === DEFAULT_ID) {
			root.removeProperty("--font-sans")
			root.removeProperty("--font-display")
		} else {
			const known = FONTS.find((f) => f.family === this.ui)
			const stack = known
				? stackOf(known)
				: `${quoted(this.ui)}, ${FALLBACK.sans}`
			root.setProperty("--font-sans", stack)
			root.setProperty("--font-display", stack)
		}

		if (this.mono === DEFAULT_ID) {
			root.removeProperty("--font-mono")
		} else {
			root.setProperty("--font-mono", `${quoted(this.mono)}, ${FALLBACK.mono}`)
		}

		// Rescaling the size tokens resizes the whole launcher at once. At 100%
		// the overrides are removed entirely so tokens.css stays in charge.
		for (const [token, base] of Object.entries(BASE_SIZES)) {
			if (this.scale === DEFAULT_SCALE) {
				root.removeProperty(token)
			} else {
				root.setProperty(token, `${Math.round((base * this.scale) / 10) / 10}px`)
			}
		}
	}
}

export const fonts = new FontState()
