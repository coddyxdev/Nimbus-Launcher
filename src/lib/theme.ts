/**
 * Thin compatibility layer over the appearance engine in `themes.svelte.ts`.
 *
 * Components that only need "dark / light / system" and an accent keep using
 * these helpers, while the theme catalogue owns the actual state. Everything
 * here delegates, so there is still exactly one place that writes `data-theme`.
 */
import type { Theme } from "./ipc"
import {
	ACCENTS as ACCENT_PRESETS,
	appearance,
	watchSystemAppearance,
	type AccentId as AccentKey,
} from "./themes.svelte"

export type AccentId = AccentKey

/** Accent catalogue, re-exported so existing imports keep working. */
export const ACCENTS = ACCENT_PRESETS

/**
 * Restores the appearance saved locally, using the launcher config only as a
 * fallback for a first run. Called on boot and after the config is reloaded.
 */
export function applyTheme(theme: Theme): void {
	appearance.hydrate(theme)
}

/**
 * Explicit dark / light / system choice from Settings: switches to the default
 * preset of that base instead of silently keeping the current catalogue theme.
 */
export function selectBaseTheme(theme: Theme): void {
	appearance.selectBase(theme)
}

/** Re-applies on OS change while `system` is selected. Returns an unsubscriber. */
export function watchSystemTheme(_getTheme?: () => Theme): () => void {
	return watchSystemAppearance()
}

/** The accent stored locally, falling back to the default. */
export function readAccent(): AccentId {
	return appearance.accentId
}

/** Writes `data-accent` on the root element and persists the choice. */
export function applyAccent(accent: AccentId): void {
	appearance.setAccent(accent)
}
