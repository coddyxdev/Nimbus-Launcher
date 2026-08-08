/**
 * DOM-level auto translation layer.
 *
 * The UI is authored in Russian. Instead of touching every single call site,
 * this walks the rendered DOM and swaps text nodes and a few user-visible
 * attributes using the RU -> EN dictionary. The original Russian is kept in a
 * WeakMap so switching back to RU restores the exact source text.
 *
 * Strings that already go through `t()` are simply no-ops here.
 */
import { EN } from "./locale-en"
import { i18n } from "./i18n.svelte"

const ATTRS = ["title", "placeholder", "aria-label", "alt"] as const
const SKIP_TAGS = new Set(["SCRIPT", "STYLE", "CODE", "PRE", "TEXTAREA"])

const originalText = new WeakMap<Text, string>()
const originalAttrs = new WeakMap<Element, Map<string, string>>()

let observer: MutationObserver | null = null
let applying = false

function toEnglish(source: string): string | null {
	const trimmed = source.trim()
	if (!trimmed) return null
	const hit = EN[trimmed]
	if (!hit || hit === trimmed) return null
	// Preserve the surrounding whitespace of the original node.
	const lead = source.slice(0, source.indexOf(trimmed))
	const tail = source.slice(source.indexOf(trimmed) + trimmed.length)
	return `${lead}${hit}${tail}`
}

function skipped(node: Node): boolean {
	const parent =
		node.nodeType === Node.ELEMENT_NODE
			? (node as Element)
			: node.parentElement
	if (!parent) return true
	if (SKIP_TAGS.has(parent.tagName)) return true
	return parent.closest("[data-no-i18n]") !== null
}

function applyText(node: Text, english: boolean) {
	if (skipped(node)) return
	const source = originalText.get(node) ?? node.nodeValue ?? ""
	if (english) {
		const translated = toEnglish(source)
		if (translated === null) return
		originalText.set(node, source)
		if (node.nodeValue !== translated) node.nodeValue = translated
	} else if (originalText.has(node)) {
		if (node.nodeValue !== source) node.nodeValue = source
		originalText.delete(node)
	}
}

function applyAttrs(el: Element, english: boolean) {
	if (SKIP_TAGS.has(el.tagName) || el.closest("[data-no-i18n]")) return
	let store = originalAttrs.get(el)
	for (const attr of ATTRS) {
		const current = el.getAttribute(attr)
		if (current === null) continue
		const source = store?.get(attr) ?? current
		if (english) {
			const translated = toEnglish(source)
			if (translated === null) continue
			if (!store) {
				store = new Map()
				originalAttrs.set(el, store)
			}
			store.set(attr, source)
			if (current !== translated) el.setAttribute(attr, translated)
		} else if (store?.has(attr)) {
			if (current !== source) el.setAttribute(attr, source)
			store.delete(attr)
		}
	}
}

function walk(root: Node, english: boolean) {
	if (root.nodeType === Node.TEXT_NODE) {
		applyText(root as Text, english)
		return
	}
	if (root.nodeType !== Node.ELEMENT_NODE) return

	applyAttrs(root as Element, english)
	const walker = document.createTreeWalker(
		root,
		NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT,
	)
	let node = walker.nextNode()
	while (node) {
		if (node.nodeType === Node.TEXT_NODE) applyText(node as Text, english)
		else applyAttrs(node as Element, english)
		node = walker.nextNode()
	}
}

function runGuarded(work: () => void) {
	if (applying) return
	applying = true
	try {
		work()
	} finally {
		// Drop the mutations we produced ourselves.
		observer?.takeRecords()
		applying = false
	}
}

/** Starts the translator. Safe to call more than once. */
export function startAutoTranslate() {
	if (typeof document === "undefined" || observer) return

	observer = new MutationObserver((records) => {
		if (applying) return
		const english = i18n.current === "en"
		runGuarded(() => {
			for (const record of records) {
				if (record.type === "characterData") {
					// Svelte replaced the text: the new value is the new original.
					originalText.delete(record.target as Text)
					applyText(record.target as Text, english)
				} else if (record.type === "attributes") {
					const el = record.target as Element
					originalAttrs.get(el)?.delete(record.attributeName ?? "")
					applyAttrs(el, english)
				} else {
					for (const added of record.addedNodes) walk(added, english)
				}
			}
		})
	})

	observer.observe(document.body, {
		subtree: true,
		childList: true,
		characterData: true,
		attributes: true,
		attributeFilter: [...ATTRS],
	})

	$effect.root(() => {
		$effect(() => {
			const english = i18n.current === "en"
			runGuarded(() => walk(document.body, english))
		})
	})
}
