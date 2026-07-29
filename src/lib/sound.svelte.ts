/**
 * Synthesized UI sound engine.
 *
 * Nimbus ships no external audio files: every effect below is composed at
 * runtime with the Web Audio API. That means there is nothing to download,
 * nothing to license, and every sound is genuinely custom to this launcher.
 * Volume and the on/off switch persist locally and are exposed to the
 * settings screen.
 */

type SoundName =
	| "hover"
	| "click"
	| "toggle"
	| "success"
	| "error"
	| "warn"
	| "toast"
	| "launch"
	| "stop"
	| "tab"
	| "open"
	| "tick"
	| "delete"

 const STORAGE_KEY = "nimbus:sound"

function readStored(): { enabled: boolean; volume: number } {
	const fallback = { enabled: true, volume: 0.45 }
	try {
		const raw = localStorage.getItem(STORAGE_KEY)
		if (!raw) return fallback
		const parsed = JSON.parse(raw) as Partial<typeof fallback>
		return {
			enabled: typeof parsed.enabled === "boolean" ? parsed.enabled : fallback.enabled,
			volume: typeof parsed.volume === "number" ? parsed.volume : fallback.volume,
		}
	} catch {
		return fallback
	}
}

class SoundEngine {
	enabled = $state(readStored().enabled)
	volume = $state(readStored().volume)

	private ctx: AudioContext | null = null
	private lastTick = 0

	private persist() {
		try {
			localStorage.setItem(
				STORAGE_KEY,
				JSON.stringify({ enabled: this.enabled, volume: this.volume }),
			)
		} catch {
			// Non-critical: worst case sound resets to defaults next launch.
		}
	}

	setEnabled(next: boolean) {
		this.enabled = next
		this.persist()
		if (next) this.play("toggle")
	}

	setVolume(next: number) {
		this.volume = Math.min(1, Math.max(0, next))
		this.persist()
	}

	private ensureContext(): AudioContext | null {
		if (typeof window === "undefined") return null
		if (!this.ctx) {
			const Ctor = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext
			if (!Ctor) return null
			this.ctx = new Ctor()
		}
		if (this.ctx.state === "suspended") void this.ctx.resume()
		return this.ctx
	}

	/** One oscillator with an amplitude envelope and optional pitch glide. */
	private tone(opts: {
		freq: number
		glideTo?: number
		type?: OscillatorType
		dur: number
		gain?: number
		delay?: number
		attack?: number
	}) {
		const ctx = this.ensureContext()
		if (!ctx) return
		const t0 = ctx.currentTime + (opts.delay ?? 0)
		const osc = ctx.createOscillator()
		const gainNode = ctx.createGain()
		osc.type = opts.type ?? "sine"
		osc.frequency.setValueAtTime(opts.freq, t0)
		if (opts.glideTo) {
			osc.frequency.exponentialRampToValueAtTime(Math.max(1, opts.glideTo), t0 + opts.dur)
		}
		const peak = (opts.gain ?? 0.18) * this.volume
		const attack = opts.attack ?? 0.008
		gainNode.gain.setValueAtTime(0.0001, t0)
		gainNode.gain.exponentialRampToValueAtTime(Math.max(0.0002, peak), t0 + attack)
		gainNode.gain.exponentialRampToValueAtTime(0.0001, t0 + opts.dur)
		osc.connect(gainNode)
		gainNode.connect(ctx.destination)
		osc.start(t0)
		osc.stop(t0 + opts.dur + 0.02)
	}

	/** Short filtered noise burst, used for percussive clicks and the "delete" thud. */
	private noise(opts: { dur: number; gain?: number; delay?: number; cutoff?: number }) {
		const ctx = this.ensureContext()
		if (!ctx) return
		const t0 = ctx.currentTime + (opts.delay ?? 0)
		const len = Math.max(1, Math.floor(ctx.sampleRate * opts.dur))
		const buffer = ctx.createBuffer(1, len, ctx.sampleRate)
		const data = buffer.getChannelData(0)
		for (let i = 0; i < len; i++) data[i] = (Math.random() * 2 - 1) * (1 - i / len)
		const src = ctx.createBufferSource()
		src.buffer = buffer
		const filter = ctx.createBiquadFilter()
		filter.type = "lowpass"
		filter.frequency.value = opts.cutoff ?? 2200
		const gainNode = ctx.createGain()
		const peak = (opts.gain ?? 0.15) * this.volume
		gainNode.gain.setValueAtTime(Math.max(0.0002, peak), t0)
		gainNode.gain.exponentialRampToValueAtTime(0.0001, t0 + opts.dur)
		src.connect(filter)
		filter.connect(gainNode)
		gainNode.connect(ctx.destination)
		src.start(t0)
	}

	play(name: SoundName) {
		if (!this.enabled || this.volume <= 0) return
		switch (name) {
			case "hover":
				this.tone({ freq: 1400, dur: 0.045, gain: 0.045, type: "sine" })
				break
			case "click":
				this.tone({ freq: 720, glideTo: 380, dur: 0.08, gain: 0.14, type: "triangle" })
				this.noise({ dur: 0.03, gain: 0.05 })
				break
			case "tab":
				this.tone({ freq: 900, glideTo: 1200, dur: 0.06, gain: 0.08 })
				break
			case "toggle":
				this.tone({ freq: 620, glideTo: 940, dur: 0.09, gain: 0.12 })
				break
			case "open":
				this.tone({ freq: 480, glideTo: 760, dur: 0.16, gain: 0.1 })
				break
			case "success":
				this.tone({ freq: 523.25, dur: 0.14, gain: 0.13 })
				this.tone({ freq: 659.25, dur: 0.16, gain: 0.13, delay: 0.08 })
				this.tone({ freq: 987.77, dur: 0.24, gain: 0.14, delay: 0.16 })
				break
			case "toast":
				this.tone({ freq: 880, dur: 0.08, gain: 0.09 })
				this.tone({ freq: 1320, dur: 0.1, gain: 0.07, delay: 0.04 })
				break
			case "warn":
				this.tone({ freq: 440, dur: 0.1, gain: 0.13, type: "triangle" })
				this.tone({ freq: 440, dur: 0.1, gain: 0.13, type: "triangle", delay: 0.14 })
				break
			case "error":
				this.tone({ freq: 260, glideTo: 110, dur: 0.28, gain: 0.16, type: "sawtooth" })
				break
			case "delete":
				this.tone({ freq: 300, glideTo: 90, dur: 0.22, gain: 0.15, type: "square" })
				this.noise({ dur: 0.12, gain: 0.12, cutoff: 900 })
				break
			case "launch":
				this.tone({ freq: 220, glideTo: 880, dur: 0.42, gain: 0.16, type: "sine" })
				this.tone({ freq: 330, glideTo: 1320, dur: 0.42, gain: 0.08, delay: 0.03 })
				break
			case "stop":
				this.tone({ freq: 660, glideTo: 220, dur: 0.26, gain: 0.14 })
				break
			case "tick": {
				// Throttled: install progress fires far more often than is audible/pleasant.
				const now = performance.now()
				if (now - this.lastTick < 220) return
				this.lastTick = now
				this.tone({ freq: 1500, dur: 0.02, gain: 0.035 })
				break
			}
		}
	}
}

export const sound = new SoundEngine()
