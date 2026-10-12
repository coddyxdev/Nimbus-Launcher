/**
 * Tests for the shipped changelog.
 *
 * `entriesSince` decides what the "What's new" dialog shows after an update.
 * Two failure modes matter: dumping the whole project history at a first-time
 * user, and showing nothing at all after a real update. The comparator is the
 * usual source of both, since it has to cope with the version strings a build
 * actually reports.
 */
import { describe, expect, it } from "vitest"

import { CHANGELOG, compareVersions, entriesSince } from "./changelog"

describe("compareVersions", () => {
	it("orders by major, minor, then patch", () => {
		expect(compareVersions("1.7.7", "1.7.5")).toBeGreaterThan(0)
		expect(compareVersions("1.7.5", "1.7.7")).toBeLessThan(0)
		expect(compareVersions("2.0.0", "1.9.9")).toBeGreaterThan(0)
	})

	it("treats equal versions as equal", () => {
		expect(compareVersions("1.7.7", "1.7.7")).toBe(0)
	})

	it("compares numerically rather than lexically", () => {
		// The classic trap: "1.7.10" sorts before "1.7.9" as a string.
		expect(compareVersions("1.7.10", "1.7.9")).toBeGreaterThan(0)
		expect(compareVersions("1.10.0", "1.9.0")).toBeGreaterThan(0)
	})

	it("treats a missing component as zero", () => {
		expect(compareVersions("1.7", "1.7.0")).toBe(0)
		expect(compareVersions("1.7", "1.7.1")).toBeLessThan(0)
	})

	it("ignores a leading v", () => {
		// Git tags carry it; `bootstrap().launcherVersion` does not.
		expect(compareVersions("v1.7.7", "1.7.7")).toBe(0)
		expect(compareVersions("V1.8.0", "1.7.7")).toBeGreaterThan(0)
	})

	it("splits on prerelease separators", () => {
		expect(compareVersions("1.7.7-beta", "1.7.7")).toBe(0)
		expect(compareVersions("1.7.7+build2", "1.7.7")).toBe(0)
	})

	it("treats an unparsable component as zero instead of NaN", () => {
		// NaN comparisons are always false, which would make the comparator
		// report "equal" for genuinely different versions.
		expect(compareVersions("1.x.7", "1.0.7")).toBe(0)
		expect(compareVersions("garbage", "0.0.0")).toBe(0)
	})

	it("is antisymmetric", () => {
		const pairs: Array<[string, string]> = [
			["1.7.7", "1.7.5"],
			["1.0.0", "2.0.0"],
			["1.7.10", "1.7.9"],
		]
		for (const [a, b] of pairs) {
			expect(Math.sign(compareVersions(a, b))).toBe(-Math.sign(compareVersions(b, a)))
		}
	})
})

describe("entriesSince", () => {
	it("shows only the current entry on a first launch", () => {
		// seen === null means the marker was never written. Returning everything
		// would greet a new user with the entire project history.
		const entries = entriesSince(null, "1.7.7")
		expect(entries).toHaveLength(1)
		expect(entries[0]!.version).toBe("1.7.7")
	})

	it("returns nothing on a first launch of a version without an entry", () => {
		expect(entriesSince(null, "1.9.0")).toHaveLength(0)
	})

	it("returns every entry released since the seen version", () => {
		const entries = entriesSince("1.7.4", "1.7.7")
		expect(entries.map((e) => e.version)).toEqual(["1.7.7", "1.7.5"])
	})

	it("returns nothing when the seen version is current", () => {
		expect(entriesSince("1.7.7", "1.7.7")).toHaveLength(0)
	})

	it("never returns an entry newer than the running build", () => {
		// A downgrade, or a changelog edited ahead of the release, must not
		// advertise features the binary does not have.
		expect(entriesSince("1.7.4", "1.7.5").map((e) => e.version)).toEqual(["1.7.5"])
	})

	it("returns nothing when the seen version is ahead of the build", () => {
		expect(entriesSince("1.9.0", "1.7.7")).toHaveLength(0)
	})

	it("preserves the newest-first order", () => {
		const entries = entriesSince("0.0.0", "1.7.7")
		for (let i = 1; i < entries.length; i += 1) {
			expect(compareVersions(entries[i - 1]!.version, entries[i]!.version)).toBeGreaterThan(0)
		}
	})
})

describe("CHANGELOG data", () => {
	it("is sorted newest first", () => {
		for (let i = 1; i < CHANGELOG.length; i += 1) {
			expect(
				compareVersions(CHANGELOG[i - 1]!.version, CHANGELOG[i]!.version),
				`entry ${i} is out of order`,
			).toBeGreaterThan(0)
		}
	})

	it("has unique versions", () => {
		const versions = CHANGELOG.map((e) => e.version)
		expect(new Set(versions).size).toBe(versions.length)
	})

	it("carries both languages for every entry", () => {
		// A missing translation renders as an empty dialog rather than falling
		// back, because the component picks the array by language directly.
		for (const entry of CHANGELOG) {
			expect(entry.titleRu.length, entry.version).toBeGreaterThan(0)
			expect(entry.titleEn.length, entry.version).toBeGreaterThan(0)
			expect(entry.itemsRu.length, entry.version).toBeGreaterThan(0)
			expect(entry.itemsEn.length, entry.version).toBeGreaterThan(0)
		}
	})

	it("uses parsable ISO dates", () => {
		for (const entry of CHANGELOG) {
			expect(entry.date, entry.version).toMatch(/^\d{4}-\d{2}-\d{2}$/)
			expect(Number.isNaN(new Date(entry.date).getTime()), entry.version).toBe(false)
		}
	})

	it("uses version strings the comparator can parse", () => {
		for (const entry of CHANGELOG) {
			expect(entry.version, entry.version).toMatch(/^\d+\.\d+(\.\d+)?$/)
		}
	})
})
