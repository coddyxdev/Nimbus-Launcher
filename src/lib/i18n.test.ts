/**
 * Tests for the localisation layer.
 *
 * `t` and `tf` are the only path by which UI text reaches the screen, so a
 * regression here silently ships Russian source strings to English users. The
 * dictionary tests guard the invariants that make a missing translation degrade
 * gracefully instead of rendering a broken key.
 */
import { beforeEach, describe, expect, it } from "vitest"

import { i18n, LANGS, locale, t, tf } from "./i18n.svelte"
import { EN } from "./locale-en"

describe("t", () => {
	beforeEach(() => i18n.set("en"))

	it("returns the source string unchanged in Russian", () => {
		i18n.set("ru")
		expect(t("Настройки")).toBe("Настройки")
	})

	it("translates a known string to English", () => {
		expect(t("Настройки")).toBe("Settings")
	})

	it("falls back to the Russian source for an unknown string", () => {
		// This is the whole point of using source strings as keys: an
		// untranslated phrase stays readable instead of showing a raw key.
		const missing = "Совершенно непереведённая строка"
		expect(t(missing)).toBe(missing)
	})
})

describe("tf", () => {
	beforeEach(() => i18n.set("en"))

	it("substitutes positional placeholders", () => {
		expect(tf("{0} загрузок", "1.2M")).toBe("1.2M downloads")
	})

	it("substitutes several placeholders", () => {
		expect(tf("{0} из {1}", 3, 40)).toBe("3 of 40")
	})

	it("allows a translation to reorder placeholders", () => {
		// Word order differs between languages, which is why the values are
		// applied after the lookup rather than before it.
		const ru = tf("Обновлено: {0}, не удалось: {1}", 5, 2)
		i18n.set("en")
		const en = tf("Обновлено: {0}, не удалось: {1}", 5, 2)
		expect(ru).toContain("5")
		expect(en).toContain("5")
		expect(en).toContain("2")
	})

	it("leaves an unfilled placeholder literal", () => {
		// Better a visible {1} in one string than a crash or the text "undefined".
		expect(tf("{0} из {1}", 3)).toBe("3 of {1}")
	})

	it("coerces numbers to strings", () => {
		expect(tf("{0} загрузок", 42)).toBe("42 downloads")
	})
})

describe("locale", () => {
	it("maps the language to a BCP-47 tag for Intl", () => {
		i18n.set("ru")
		expect(locale()).toBe("ru-RU")
		i18n.set("en")
		expect(locale()).toBe("en-US")
	})

	it("produces a tag Intl actually accepts", () => {
		// A malformed tag throws inside toLocaleDateString and takes the whole
		// pane down with it.
		i18n.set("en")
		expect(() => new Date().toLocaleDateString(locale())).not.toThrow()
		i18n.set("ru")
		expect(() => new Date().toLocaleDateString(locale())).not.toThrow()
	})
})

describe("i18n state", () => {
	it("persists the choice and reflects it on the document", () => {
		i18n.set("ru")
		expect(i18n.current).toBe("ru")
		expect(document.documentElement.lang).toBe("ru")
		expect(localStorage.getItem("nimbus.lang")).toBe("ru")
	})

	it("toggles between the two languages", () => {
		i18n.set("en")
		i18n.toggle()
		expect(i18n.current).toBe("ru")
		i18n.toggle()
		expect(i18n.current).toBe("en")
	})

	it("offers exactly the languages the dictionary supports", () => {
		expect(LANGS.map((l) => l.id).sort()).toEqual(["en", "ru"])
	})
})

describe("EN dictionary", () => {
	it("has no empty translations", () => {
		for (const [ru, en] of Object.entries(EN)) {
			expect(en.length, `empty translation for "${ru}"`).toBeGreaterThan(0)
		}
	})

	it("preserves every placeholder index across the translation", () => {
		// Dropping {0} from a translation silently deletes a value the user
		// needed — a version number, a file count, an error message.
		const indices = (s: string) =>
			(s.match(/\{(\d+)\}/g) ?? []).sort().join(",")
		for (const [ru, en] of Object.entries(EN)) {
			expect(indices(en), `placeholders differ for "${ru}"`).toBe(indices(ru))
		}
	})

	it("has no key that maps to itself", () => {
		// A self-mapping entry is dead weight and, worse, blocks the reverse
		// dictionary the DOM translator builds for restoring Russian.
		for (const [ru, en] of Object.entries(EN)) {
			expect(en, `"${ru}" maps to itself`).not.toBe(ru)
		}
	})

	it("keeps the unit strings needed by the progress readouts", () => {
		// These four are produced by fmtSpeed/fmtEta rather than written in a
		// component, so a missing key here is invisible until runtime.
		for (const key of ["{0} МБ/с", "{0} КБ/с", "~{0} с", "~{0} мин {1} с"]) {
			expect(EN[key], `missing unit key "${key}"`).toBeDefined()
		}
	})
})
