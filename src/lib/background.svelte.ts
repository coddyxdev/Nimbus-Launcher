/**
 * Custom launcher background: state, persistence and the CSS variables the
 * background layer and the translucency rules read.
 *
 * The picked file is copied into the launcher profile by the backend, so all
 * this module ever holds is a path plus two numbers. Nothing here decodes or
 * transfers image bytes.
 *
 * Two things are deliberate:
 *  - The slider writes to memory on every frame but to disk at most a few
 *    times a second. Dragging from 1 to 100 must feel instant and must not
 *    produce 100 config writes.
 *  - Opacity and blur are applied as CSS variables on <html>, so a change
 *    repaints without re-rendering any Svelte component.
 */

import { convertFileSrc } from "@tauri-apps/api/core"
import { open } from "@tauri-apps/plugin-dialog"

import { ipc, type BackgroundInfo, type Config } from "$lib/ipc"

/** Mirrors the limits enforced in `commands/background.rs`. */
export const MAX_IMAGE_MIB = 25
export const MAX_VIDEO_MIB = 60

export const IMAGE_FORMATS = ["png", "jpg", "jpeg", "gif", "webp"]
export const VIDEO_FORMATS = ["mp4", "webm"]

/** How long the slider may keep writing to memory before we persist. */
const SAVE_DELAY_MS = 300

function clamp(value: number, min: number, max: number): number {
	if (!Number.isFinite(value)) return min
	return Math.min(max, Math.max(min, Math.round(value)))
}

class BackgroundStore {
	/** The file in use, or null for the plain themed canvas. */
	info = $state<BackgroundInfo | null>(null)
	/** 1..100. How strongly the picture shows through. */
	opacity = $state(55)
	/** 0..40 px. Blur is what keeps text readable over a busy photo. */
	blur = $state(0)
	/** True while a file is being copied into the profile. */
	busy = $state(false)

	#saveTimer: ReturnType<typeof setTimeout> | null = null

	get active(): boolean {
		return this.info !== null
	}

	get kind(): "image" | "video" | null {
		if (!this.info) return null
		return this.info.kind === "video" ? "video" : "image"
	}

	/** Asset-protocol URL for the <img>/<video> element. */
	get src(): string | null {
		return this.info ? convertFileSrc(this.info.path) : null
	}

	get sizeLabel(): string {
		if (!this.info) return ""
		const mib = this.info.sizeBytes / (1024 * 1024)
		return mib < 0.1 ? "меньше 0,1 МБ" : `${mib.toFixed(1)} МБ`
	}

	/**
	 * Pushes the current values onto <html>.
	 *
	 * `data-bg` drives the translucency rules in app.css; without a background
	 * the attribute is absent and every surface stays fully opaque, so themes
	 * behave exactly as they did before this feature existed.
	 */
	apply(): void {
		if (typeof document === "undefined") return
		const root = document.documentElement

		if (this.active) {
			root.setAttribute("data-bg", this.kind ?? "image")
		} else {
			root.removeAttribute("data-bg")
		}

		root.style.setProperty("--bg-media-opacity", String(this.opacity / 100))

		// A CSS filter forces the picture onto its own rasterised layer, and an
		// upscale resamples every pixel. Both soften the image, so while blur is
		// off we emit literal no-ops instead of "blur(0px)" and "scale(1.0x)" —
		// the media then draws at its native resolution.
		if (this.blur > 0) {
			root.style.setProperty("--bg-media-filter", `blur(${this.blur}px)`)
			// Blur samples past the edges, so grow just enough to hide the
			// washed-out border it would otherwise leave.
			root.style.setProperty("--bg-media-scale", String(1 + this.blur / 190))
		} else {
			root.style.setProperty("--bg-media-filter", "none")
			root.style.setProperty("--bg-media-scale", "1")
		}
		// Surfaces stay readable by keeping a floor under their own opacity: the
		// stronger the picture, the more the panels let it through, but never
		// past the point where text loses its backing.
		const veil = 1 - (this.opacity / 100) * 0.42
		root.style.setProperty("--bg-surface-alpha", veil.toFixed(3))
		root.style.setProperty("--bg-surface-veil", `${Math.round(veil * 100)}%`)
	}

	/** Reads the persisted numbers, then resolves the file on disk. */
	async hydrate(config: Config): Promise<void> {
		this.opacity = clamp(config.backgroundOpacity ?? 55, 1, 100)
		this.blur = clamp(config.backgroundBlur ?? 0, 0, 40)
		this.apply()
		await this.refresh()
	}

	/** Re-resolves the background file; a missing file clears itself. */
	async refresh(): Promise<void> {
		try {
			this.info = await ipc.getBackground()
		} catch {
			this.info = null
		}
		this.apply()
	}

	/**
	 * Opens the native picker and imports the chosen file.
	 * Returns an error message, or null when it worked.
	 */
	async pick(): Promise<string | null> {
		const selected = await open({
			multiple: false,
			directory: false,
			filters: [
				{
					name: "Изображения и видео",
					extensions: [...IMAGE_FORMATS, ...VIDEO_FORMATS],
				},
				{ name: "Изображения", extensions: IMAGE_FORMATS },
				{ name: "Видео", extensions: VIDEO_FORMATS },
			],
		})
		if (typeof selected !== "string") return null
		return await this.use(selected)
	}

	/** Imports a file by absolute path — shared by the picker and drag & drop. */
	async use(path: string): Promise<string | null> {
		this.busy = true
		try {
			this.info = await ipc.setBackground(path)
			this.apply()
			return null
		} catch (err) {
			const message =
				typeof err === "object" && err !== null && "message" in err
					? String((err as { message: unknown }).message)
					: "Не удалось поставить этот файл фоном"
			return message
		} finally {
			this.busy = false
		}
	}

	/** Removes the background and its copy inside the profile. */
	async remove(): Promise<void> {
		this.info = null
		this.apply()
		try {
			await ipc.clearBackground()
		} catch {
			// The picture is already gone from the UI; a failed cleanup is not
			// worth an error dialog, the next import prunes the folder anyway.
		}
	}

	setOpacity(value: number): void {
		this.opacity = clamp(value, 1, 100)
		this.apply()
		this.#scheduleSave()
	}

	setBlur(value: number): void {
		this.blur = clamp(value, 0, 40)
		this.apply()
		this.#scheduleSave()
	}

	/** Trailing-edge save: one config write per drag, not one per frame. */
	#scheduleSave(): void {
		if (this.#saveTimer) clearTimeout(this.#saveTimer)
		this.#saveTimer = setTimeout(() => {
			this.#saveTimer = null
			void ipc
				.updateConfig({
					backgroundOpacity: this.opacity,
					backgroundBlur: this.blur,
				})
				.catch(() => {
					/* keeps the UI responsive; the next change retries */
				})
		}, SAVE_DELAY_MS)
	}
}

export const background = new BackgroundStore()
