/**
 * Tiny runtime localisation layer.
 *
 * The source strings in the components stay Russian and act as the keys, so a
 * missing translation degrades to readable text instead of a raw key. English
 * is the default on a fresh install; the choice is persisted in localStorage so
 * it survives restarts without a round trip to the Rust config.
 */
import { EN } from "./locale-en"

export type Lang = "en" | "ru"

export const LANGS: Array<{ id: Lang; label: string }> = [
	{ id: "en", label: "English" },
	{ id: "ru", label: "Русский" },
]

const STORAGE_KEY = "nimbus.lang"

function readStored(): Lang {
	try {
		const raw = localStorage.getItem(STORAGE_KEY)
		if (raw === "ru" || raw === "en") return raw
	} catch {
		// Private mode / disabled storage: fall through to the default.
	}
	// First launch is English by default, as requested.
	return "en"
}

class I18nState {
	current = $state<Lang>(readStored())

	set(lang: Lang) {
		this.current = lang
		try {
			localStorage.setItem(STORAGE_KEY, lang)
		} catch {
			// Not fatal: the UI still switches for this session.
		}
		if (typeof document !== "undefined") document.documentElement.lang = lang
	}

	toggle() {
		this.set(this.current === "ru" ? "en" : "ru")
	}
}

export const i18n = new I18nState()

if (typeof document !== "undefined") document.documentElement.lang = i18n.current

/**
 * Translates a Russian source string. Reading `i18n.current` here is what makes
 * every call site re-render when the language changes.
 */
export function t(ru: string): string {
	if (i18n.current === "ru") return ru
	return EN[ru] ?? ru
}

/**
 * Same as `t`, with `{0}`, `{1}` … placeholders filled in afterwards, so word
 * order can differ between languages.
 */
export function tf(ru: string, ...values: Array<string | number>): string {
	return t(ru).replace(/\{(\d+)\}/g, (match, index: string) => {
		const value = values[Number(index)]
		return value === undefined ? match : String(value)
	})
}

/** BCP-47 tag for `Intl` formatting (numbers, dates). */
export function locale(): string {
	return i18n.current === "ru" ? "ru-RU" : "en-US"
}
