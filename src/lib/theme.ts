import type { Theme } from "./ipc";

const media = window.matchMedia("(prefers-color-scheme: light)");

function resolve(theme: Theme): "dark" | "light" {
  if (theme === "system") {
    return media.matches ? "light" : "dark";
  }
  return theme;
}

/**
 * Applies the theme to the document root. Kept out of components so there is a
 * single place that writes `data-theme`.
 */
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = resolve(theme);
}

/** Re-applies on OS change while `system` is selected. Returns an unsubscriber. */
export function watchSystemTheme(getTheme: () => Theme): () => void {
  const handler = () => {
    if (getTheme() === "system") {
      applyTheme("system");
    }
  };
  media.addEventListener("change", handler);
  return () => media.removeEventListener("change", handler);
}

// ─── Accent colour ──────────────────────────────────────────────────────────

/**
 * Selectable accent hues. The whole UI derives its interactive colour from a
 * single `--accent*` group, so switching one attribute on `<html>` restyles the
 * entire launcher without touching component code.
 */
export const ACCENTS = [
  { id: "jade", label: "Нефрит" },
  { id: "azure", label: "Лазурь" },
  { id: "amber", label: "Янтарь" },
  { id: "crimson", label: "Багрянец" },
  { id: "violet", label: "Аметист" },
] as const;

export type AccentId = (typeof ACCENTS)[number]["id"];

const ACCENT_KEY = "nimbus.accent";
const DEFAULT_ACCENT: AccentId = "jade";

function isAccent(value: string | null): value is AccentId {
  return ACCENTS.some((a) => a.id === value);
}

/** The accent stored locally, falling back to the default. */
export function readAccent(): AccentId {
  try {
    const stored = window.localStorage.getItem(ACCENT_KEY);
    return isAccent(stored) ? stored : DEFAULT_ACCENT;
  } catch {
    return DEFAULT_ACCENT;
  }
}

/** Writes `data-accent` on the root element and persists the choice. */
export function applyAccent(accent: AccentId): void {
  document.documentElement.dataset.accent = accent;
  try {
    window.localStorage.setItem(ACCENT_KEY, accent);
  } catch {
    // Non-critical: the accent simply resets on next launch.
  }
}
