/**
 * Global toast queue.
 *
 * Lives outside the component tree so any pane can report a result without
 * threading callbacks up to App.svelte, and so a toast survives switching
 * tabs (the component that raised it may unmount immediately after).
 */

export type ToastKind = "info" | "success" | "error"

export type Toast = {
	id: number
	kind: ToastKind
	text: string
}

/** Errors stay longer: they usually carry text worth reading. */
const TTL_MS: Record<ToastKind, number> = {
	info: 3500,
	success: 2500,
	error: 8000,
}

/** Older toasts are dropped so the stack cannot cover the window. */
const MAX_VISIBLE = 4

class ToastStore {
	items = $state<Toast[]>([])
	private nextId = 1
	private timers = new Map<number, ReturnType<typeof setTimeout>>()

	push(kind: ToastKind, text: string, ttlMs?: number): number {
		const id = this.nextId++
		const next = [...this.items, { id, kind, text }]
		this.items =
			next.length > MAX_VISIBLE ? next.slice(next.length - MAX_VISIBLE) : next
		const ttl = ttlMs ?? TTL_MS[kind]
		if (ttl > 0) {
			this.timers.set(
				id,
				setTimeout(() => this.dismiss(id), ttl),
			)
		}
		return id
	}

	info(text: string) {
		return this.push("info", text)
	}

	success(text: string) {
		return this.push("success", text)
	}

	error(text: string) {
		return this.push("error", text)
	}

	dismiss(id: number) {
		const timer = this.timers.get(id)
		if (timer) {
			clearTimeout(timer)
			this.timers.delete(id)
		}
		this.items = this.items.filter((t) => t.id !== id)
	}

	clear() {
		for (const timer of this.timers.values()) clearTimeout(timer)
		this.timers.clear()
		this.items = []
	}
}

export const toasts = new ToastStore()
