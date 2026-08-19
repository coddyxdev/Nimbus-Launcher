/**
 * Release notes shipped with the binary.
 *
 * Unlike the news feed (which is fetched from the repository at runtime), the
 * changelog describes the build the user is actually running, so it has to
 * travel with it. Both languages live side by side: switching the launcher
 * language must not require another lookup.
 *
 * Keep the list newest-first; `entriesSince()` relies on the order only for
 * presentation, but a sorted list is far easier to maintain by hand.
 */

export type ChangelogEntry = {
	/** Exactly the version string reported by `bootstrap().launcherVersion`. */
	version: string
	/** ISO date, shown as-is. */
	date: string
	titleRu: string
	titleEn: string
	/** Short bullet points, one line each. */
	itemsRu: string[]
	itemsEn: string[]
}

export const CHANGELOG: ChangelogEntry[] = [
	{
		version: "1.7.7",
		date: "2026-08-19",
		titleRu: "Обновление акцентов и градиентов",
		titleEn: "Accents and Gradients Overhaul",
		itemsRu: [
			"Расширенная палитра классических акцентов: Неоновый лайм, Электрик, Лаванда, Слива, Лёд и Мандарин.",
			"Большая коллекция уникальных градиентных акцентов: Синтвейв, Инферно, Сверхновая, Ледник, Цветение сакуры, Фантом, Пламя дракона и др.",
			"Интеграция Ely.by и поддержка скинов в менеджере аккаунтов.",
			"Оптимизация тем, переключателей оформления и визуальных стилей.",
		],
		itemsEn: [
			"Expanded classic accent palette: Neon Lime, Electric, Lavender, Plum, Ice, and Mandarin.",
			"Rich collection of unique gradient accents: Synthwave, Inferno, Supernova, Glacier, Sakura Bloom, Phantom, Dragon Blaze, etc.",
			"Ely.by integration and skin preview in account manager.",
			"Performance improvements and polish across theme engine and launcher styles.",
		],
	},
	{
		version: "1.7.5",
		date: "2026-08-10",
		titleRu: "Новости и «Что нового»",
		titleEn: "News and «What's new»",
		itemsRu: [
			"Раздел «Новости» в боковой панели: лента обновляется без выпуска новой версии.",
			"Окно «Что нового» показывается один раз после обновления лаунчера.",
			"Ссылки из новостей открываются в системном браузере.",
		],
		itemsEn: [
			"A «News» section in the sidebar: the feed updates without a new release.",
			"The «What's new» dialog appears once after the launcher updates.",
			"Links from news posts open in the system browser.",
		],
	},
]

/** Parses "1.7.4" into comparable numbers; unknown parts count as 0. */
function parts(version: string): number[] {
	return version
		.trim()
		.replace(/^v/i, "")
		.split(/[.\-+]/)
		.map((piece) => Number.parseInt(piece, 10))
		.map((value) => (Number.isFinite(value) ? value : 0))
}

/** Semver-ish comparison. Returns <0, 0 or >0 like every other comparator. */
export function compareVersions(a: string, b: string): number {
	const left = parts(a)
	const right = parts(b)
	const length = Math.max(left.length, right.length)
	for (let i = 0; i < length; i += 1) {
		const diff = (left[i] ?? 0) - (right[i] ?? 0)
		if (diff !== 0) return diff
	}
	return 0
}

/**
 * Entries newer than `seen`, newest first.
 *
 * `seen` being null means "first launch we know of": only the entry for the
 * running version is returned, so a fresh install does not dump the entire
 * project history into the user's face.
 */
export function entriesSince(seen: string | null, current: string): ChangelogEntry[] {
	const known = CHANGELOG.filter((entry) => compareVersions(entry.version, current) <= 0)
	if (seen === null) {
		return known.filter((entry) => compareVersions(entry.version, current) === 0)
	}
	return known.filter((entry) => compareVersions(entry.version, seen) > 0)
}
