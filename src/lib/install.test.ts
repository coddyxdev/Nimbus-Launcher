/**
 * Tests for the shared install-progress helpers.
 *
 * `finishInstall` is the error funnel every install screen routes through, so a
 * throw inside it masks the real failure the user needed to see. The formatters
 * feed the progress readouts and must never emit a partial or misleading unit.
 */
import { beforeEach, describe, expect, it } from "vitest"

import { i18n } from "./i18n.svelte"
import { finishInstall, fmtEta, fmtSpeed, installState } from "./install.svelte"

const KIB = 1024
const MIB = 1024 * 1024

describe("fmtSpeed", () => {
	beforeEach(() => i18n.set("en"))

	it("returns an empty string when the speed is unknown", () => {
		// The caller joins the parts with " · ", so an empty string is how a
		// missing value disappears from the line instead of showing "0 KB/s".
		expect(fmtSpeed(null)).toBe("")
		expect(fmtSpeed(0)).toBe("")
		expect(fmtSpeed(-1)).toBe("")
	})

	it("uses KB/s below one mebibyte", () => {
		expect(fmtSpeed(512 * KIB)).toBe("512 KB/s")
	})

	it("switches to MB/s at one mebibyte", () => {
		expect(fmtSpeed(MIB)).toBe("1.0 MB/s")
		expect(fmtSpeed(Math.round(12.34 * MIB))).toBe("12.3 MB/s")
	})

	it("rounds kibibytes to whole numbers", () => {
		expect(fmtSpeed(1536)).toBe("2 KB/s")
	})

	it("localises the unit", () => {
		i18n.set("ru")
		expect(fmtSpeed(MIB)).toBe("1.0 МБ/с")
		expect(fmtSpeed(2 * KIB)).toBe("2 КБ/с")
	})
})

describe("fmtEta", () => {
	beforeEach(() => i18n.set("en"))

	it("returns an empty string when the ETA cannot be estimated", () => {
		expect(fmtEta(null)).toBe("")
		expect(fmtEta(-1)).toBe("")
	})

	it("shows plain seconds under a minute", () => {
		expect(fmtEta(0)).toBe("~0s")
		expect(fmtEta(59)).toBe("~59s")
	})

	it("pads the seconds component so the readout does not jitter", () => {
		// The row is rendered with tabular numerals; an unpadded "1m 5s" would
		// change width every tick.
		expect(fmtEta(65)).toBe("~1m 05s")
		expect(fmtEta(125)).toBe("~2m 05s")
	})

	it("switches to hours at sixty minutes", () => {
		expect(fmtEta(3600)).toBe("~1h 00m")
		expect(fmtEta(3600 + 25 * 60)).toBe("~1h 25m")
	})

	it("localises the units", () => {
		i18n.set("ru")
		expect(fmtEta(30)).toBe("~30 с")
		expect(fmtEta(90)).toBe("~1 мин 30 с")
	})
})

describe("finishInstall", () => {
	beforeEach(() => {
		i18n.set("en")
		installState.begin("1.20.1", "Test build")
	})

	it("clears the state on success", () => {
		finishInstall()
		expect(installState.busy).toBe(false)
		expect(installState.error).toBeNull()
	})

	it("does not throw on null", () => {
		// Regression: reading `.message` off null raised a TypeError, which
		// replaced the failure being reported with an unrelated crash.
		expect(() => finishInstall(null)).not.toThrow()
		expect(installState.error).toBeNull()
		expect(installState.busy).toBe(false)
	})

	it("swallows a user-requested cancellation", () => {
		// A cancel is not a failure and must not be shown in red.
		finishInstall({ kind: "cancelled", message: "cancelled" })
		expect(installState.error).toBeNull()
		expect(installState.busy).toBe(false)
	})

	it("surfaces a backend error message", () => {
		finishInstall({ kind: "network", message: "connection reset" })
		expect(installState.error).toBe("connection reset")
		expect(installState.busy).toBe(false)
	})

	it("stringifies an error without a message field", () => {
		finishInstall("plain failure")
		expect(installState.error).toBe("plain failure")
	})
})

describe("installState", () => {
	beforeEach(() => installState.finish())

	it("is idle until an install begins", () => {
		expect(installState.busy).toBe(false)
		expect(installState.pct).toBe(0)
		expect(installState.etaSeconds).toBeNull()
	})

	it("reports zero percent before the backend sends any totals", () => {
		installState.begin("1.20.1", "Test build")
		expect(installState.busy).toBe(true)
		expect(installState.pct).toBe(0)
	})

	it("prefers byte totals over file counts for the percentage", () => {
		// Bytes are what the user perceives as progress: 9 tiny files plus one
		// large jar is not 90% done after nine files.
		installState.begin("1.20.1", "Test build")
		installState.progress = {
			stage: "libraries",
			done: 9,
			total: 10,
			bytesDone: 25,
			bytesTotal: 100,
			file: "",
		}
		expect(installState.pct).toBe(25)
	})

	it("falls back to file counts when byte totals are absent", () => {
		installState.begin("1.20.1", "Test build")
		installState.progress = {
			stage: "assets",
			done: 3,
			total: 4,
			bytesDone: 0,
			bytesTotal: 0,
			file: "",
		}
		expect(installState.pct).toBe(75)
	})

	it("clamps a cancel request to an active install", () => {
		expect(installState.cancelling).toBe(false)
		void installState.cancel()
		// Nothing is running, so no IPC call and no stuck "Cancelling…" label.
		expect(installState.cancelling).toBe(false)
	})

	it("clears a previous error when a new install begins", () => {
		finishInstall({ message: "old failure" })
		expect(installState.error).toBe("old failure")
		installState.begin("1.21.1", "Another build")
		expect(installState.error).toBeNull()
	})

	it("clears the error on demand", () => {
		finishInstall({ message: "shown once" })
		installState.clearError()
		expect(installState.error).toBeNull()
	})
})
