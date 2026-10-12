/**
 * Vitest setup.
 *
 * Several stores subscribe to Tauri IPC at import time (`installState`
 * subscribes to `install:progress`, for example). Outside a WebView the Tauri
 * bridge is absent, so those calls reject and print a stack trace that buries
 * the actual test output. Stubbing the two transport modules keeps the pure
 * logic under test reachable without pulling in a real backend.
 */
import { vi } from "vitest"

vi.mock("@tauri-apps/api/event", () => ({
	listen: () => Promise.resolve(() => {}),
	emit: () => Promise.resolve(),
	once: () => Promise.resolve(() => {}),
}))

vi.mock("@tauri-apps/api/core", () => ({
	invoke: () => Promise.reject(new Error("IPC is not available under test")),
	convertFileSrc: (path: string) => path,
}))
