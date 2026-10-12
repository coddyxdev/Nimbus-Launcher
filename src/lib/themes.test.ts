/**
 * Tests for the appearance engine.
 *
 * `sanitizeCss` is the security-relevant part: user-supplied theme CSS is
 * interpolated into a `<style>` element, so each escaping test below stands for
 * a way a shared theme file could exfiltrate data or break out of the style
 * block. The rest covers the colour maths and preset invariants that silently
 * produce an unreadable UI when they drift.
 */
import { describe, expect, it } from "vitest"

import {
	ACCENTS,
	BASE_VARS,
	PRESETS,
	SYSTEM_ID,
	presetToCss,
	readCssVar,
	sanitizeCss,
} from "./themes.svelte"

describe("sanitizeCss", () => {
	it("strips a closing style tag", () => {
		// Otherwise the theme escapes its own <style> element and can inject
		// arbitrary markup into the document.
		const out = sanitizeCss("</style><script>alert(1)</script>")
		expect(out).not.toContain("</style")
		expect(out).not.toContain("<style")
	})

	it("strips an opening style tag regardless of case", () => {
		expect(sanitizeCss("<STYLE>x</STYLE>")).not.toMatch(/<\/?style/i)
	})

	it("strips @import", () => {
		// @import pulls in remote CSS, which is both a network beacon and a way
		// to load rules this function never inspected.
		const out = sanitizeCss('@import url("https://evil.example/x.css");')
		expect(out).not.toContain("@import")
	})

	it("strips @charset", () => {
		expect(sanitizeCss('@charset "utf-8"; :root{}')).not.toContain("@charset")
	})

	it("defuses IE expression()", () => {
		expect(sanitizeCss("width: expression(alert(1));")).not.toContain("expression(")
	})

	it("strips the javascript: scheme", () => {
		expect(sanitizeCss("background: javascript:alert(1);")).not.toContain("javascript:")
	})

	it("empties url() pointing at an https host", () => {
		const out = sanitizeCss("background: url(https://evil.example/track.png);")
		expect(out).toContain("url()")
		expect(out).not.toContain("evil.example")
	})

	it("empties a protocol-relative url()", () => {
		// Regression: stripping the scheme alone leaves `url(//host/x)`, which is
		// still a valid remote fetch.
		const out = sanitizeCss("background: url(//evil.example/track.png);")
		expect(out).toContain("url()")
		expect(out).not.toContain("evil.example")
	})

	it("empties a quoted url()", () => {
		expect(sanitizeCss("background: url('https://evil.example/x')")).toContain("url()")
		expect(sanitizeCss('background: url("https://evil.example/x")')).toContain("url()")
	})

	it("keeps an inline data:image payload", () => {
		// The one allowed case: an embedded image cannot phone home.
		const css = "background: url(data:image/png;base64,iVBORw0KGgo=);"
		expect(sanitizeCss(css)).toContain("data:image/png")
	})

	it("rejects a non-image data URI", () => {
		const out = sanitizeCss("background: url(data:text/html;base64,PHNjcmlwdD4=)")
		expect(out).toContain("url()")
		expect(out).not.toContain("text/html")
	})

	it("preserves ordinary custom properties", () => {
		const css = ":root{--bg-canvas:#101010;--text-primary:#fff}"
		expect(sanitizeCss(css)).toBe(css)
	})

	it("trims surrounding whitespace", () => {
		expect(sanitizeCss("\n  :root{}  \n")).toBe(":root{}")
	})

	it("returns an empty string for empty input", () => {
		expect(sanitizeCss("")).toBe("")
		expect(sanitizeCss("   ")).toBe("")
	})
})

describe("readCssVar", () => {
	it("reads a declared token", () => {
		expect(readCssVar(":root{--bg-canvas:#0a0a0c;}", "bg-canvas")).toBe("#0a0a0c")
	})

	it("tolerates loose whitespace", () => {
		expect(readCssVar(":root{ --bg-canvas :  #fff ; }", "bg-canvas")).toBe("#fff")
	})

	it("returns null for a missing token", () => {
		expect(readCssVar(":root{--other:#fff}", "bg-canvas")).toBeNull()
	})
})

describe("presetToCss", () => {
	it("emits every token the preview needs", () => {
		const css = presetToCss(PRESETS[1]!)
		for (const name of [
			"--bg-canvas",
			"--bg-surface",
			"--bg-raised",
			"--bg-hover",
			"--bg-active",
			"--bg-inset",
			"--text-primary",
			"--text-secondary",
			"--text-tertiary",
			"--text-disabled",
			"--skeleton-base",
		]) {
			expect(css).toContain(name)
		}
	})

	it("falls back to the base palette for a preset without overrides", () => {
		// `system` and the two defaults carry vars: null, meaning "use
		// tokens.css as-is"; duplicating one must still yield a usable theme.
		const system = PRESETS.find((p) => p.id === SYSTEM_ID)!
		expect(system.vars).toBeNull()
		expect(presetToCss(system)).toContain(BASE_VARS.dark.canvas)
	})

	it("round-trips through readCssVar", () => {
		const preset = PRESETS.find((p) => p.id === "carbon")!
		const css = presetToCss(preset)
		expect(readCssVar(css, "bg-canvas")).toBe(preset.vars!.canvas)
	})

	it("survives sanitizeCss unchanged in substance", () => {
		// A duplicated preset goes straight into the custom-theme editor, whose
		// save path sanitizes. Nothing a preset emits may be stripped.
		const css = presetToCss(PRESETS.find((p) => p.id === "nord")!)
		expect(readCssVar(sanitizeCss(css), "bg-canvas")).toBe(readCssVar(css, "bg-canvas"))
	})
})

describe("preset catalogue", () => {
	it("has unique ids", () => {
		const ids = PRESETS.map((p) => p.id)
		expect(new Set(ids).size).toBe(ids.length)
	})

	it("references an accent that exists", () => {
		const accents = new Set(ACCENTS.map((a) => a.id))
		for (const preset of PRESETS) {
			expect(accents, `preset ${preset.id}`).toContain(preset.accent)
		}
	})

	it("declares every colour token when it overrides the base", () => {
		const required = Object.keys(BASE_VARS.dark) as Array<keyof typeof BASE_VARS.dark>
		for (const preset of PRESETS) {
			if (!preset.vars) continue
			for (const key of required) {
				expect(preset.vars[key], `${preset.id}.${key}`).toMatch(/^#[0-9a-f]{6}$/i)
			}
		}
	})

	it("keeps system first so it is the default selection", () => {
		expect(PRESETS[0]!.id).toBe(SYSTEM_ID)
	})
})

describe("accent catalogue", () => {
	it("has unique ids", () => {
		const ids = ACCENTS.map((a) => a.id)
		expect(new Set(ids).size).toBe(ids.length)
	})

	it("defines a valid hex for both bases", () => {
		for (const accent of ACCENTS) {
			expect(accent.dark, `${accent.id}.dark`).toMatch(/^#[0-9a-f]{6}$/i)
			expect(accent.light, `${accent.id}.light`).toMatch(/^#[0-9a-f]{6}$/i)
		}
	})

	it("gives every gradient accent both gradient variants", () => {
		for (const accent of ACCENTS) {
			if (accent.kind !== "gradient") continue
			expect(accent.gradient, `${accent.id}`).toBeDefined()
			expect(accent.gradient!.dark).toContain("gradient(")
			expect(accent.gradient!.light).toContain("gradient(")
		}
	})

	it("keeps jade as the default accent", () => {
		expect(ACCENTS[0]!.id).toBe("jade")
	})
})
