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
/**
 * Selector, not a tag set: text lives in wrappers as often as not, so
 * `<pre><span>…</span></pre>` and `<code><em>…</em></code>` have to be skipped
 * by ancestry rather than by the immediate parent's tag name.
 */
const SKIP_SELECTOR = "script,style,code,pre,textarea,[data-no-i18n]"

const originalText = new WeakMap<Text, string>()
const originalAttrs = new WeakMap<Element, Map<string, string>>()

let observer: MutationObserver | null = null
let applying = false
let disposeEffect: (() => void) | null = null

/**
 * Nodes and attributes this module wrote during the current pass.
 *
 * The observer needs to ignore its own edits without discarding the whole
 * mutation queue: genuine Svelte updates routinely land in the same batch, and
 * flushing them wholesale left freshly rendered Russian text untranslated.
 */
const selfText = new Set<Text>()
const selfAttrs = new Set<string>()
let selfKeyed = new WeakMap<Element, number>()
let selfSeq = 0

function attrKey(el: Element, attr: string): string {
	let id = selfKeyed.get(el)
	if (id === undefined) {
		id = ++selfSeq
		selfKeyed.set(el, id)
	}
	return `${id}:${attr}`
}

/**
 * EN -> RU lookup, built once from the forward dictionary.
 *
 * Restoring Russian used to rely solely on the WeakMap of originals, which is
 * lost whenever this module is re-evaluated (dev reloads) or a node is
 * re-created outside a mutation we saw. The result was a UI stuck in English
 * with no way back. Translating in reverse makes the switch self-healing.
 *
 * English phrases produced by more than one Russian source are dropped: there
 * is no way to pick the right original, and guessing would corrupt the text.
 */
let reverse: Map<string, string> | null = null

function reverseDict(): Map<string, string> {
	if (reverse) return reverse
	const counts = new Map<string, number>()
	for (const en of Object.values(EN)) {
		counts.set(en, (counts.get(en) ?? 0) + 1)
	}
	reverse = new Map()
	for (const [ru, en] of Object.entries(EN)) {
		if (en === ru || counts.get(en) !== 1) continue
		reverse.set(en, ru)
	}
	return reverse
}

/** Swaps a string through a lookup while keeping the node's own whitespace. */
function swap(source: string, lookup: (key: string) => string | undefined): string | null {
	const trimmed = source.trim()
	if (!trimmed) return null
	const hit = lookup(trimmed)
	if (!hit || hit === trimmed) return null
	const lead = source.slice(0, source.indexOf(trimmed))
	const tail = source.slice(source.indexOf(trimmed) + trimmed.length)
	return `${lead}${hit}${tail}`
}

function toEnglish(source: string): string | null {
	return swap(source, (key) => EN[key])
}

function toRussian(source: string): string | null {
	return swap(source, (key) => reverseDict().get(key))
}

function skipped(node: Node): boolean {
	const parent =
		node.nodeType === Node.ELEMENT_NODE
			? (node as Element)
			: node.parentElement
	if (!parent) return true
	return parent.closest(SKIP_SELECTOR) !== null
}

/** Writes a text node and records the edit so the observer can ignore it. */
function writeText(node: Text, value: string) {
	if (node.nodeValue === value) return
	node.nodeValue = value
	selfText.add(node)
}

/** Writes an attribute and records the edit so the observer can ignore it. */
function writeAttr(el: Element, attr: string, value: string) {
	if (el.getAttribute(attr) === value) return
	el.setAttribute(attr, value)
	selfAttrs.add(attrKey(el, attr))
}

function applyText(node: Text, english: boolean) {
	if (skipped(node)) return
	const source = originalText.get(node) ?? node.nodeValue ?? ""
	if (english) {
		const translated = toEnglish(source)
		if (translated === null) return
		originalText.set(node, source)
		writeText(node, translated)
	} else if (originalText.has(node)) {
		writeText(node, source)
		originalText.delete(node)
	} else {
		// Nothing remembered for this node: translate back instead of leaving
		// English on screen.
		const restored = toRussian(source)
		if (restored !== null) writeText(node, restored)
	}
}

function applyAttrs(el: Element, english: boolean) {
	if (el.closest(SKIP_SELECTOR)) return
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
			writeAttr(el, attr, translated)
		} else if (store?.has(attr)) {
			writeAttr(el, attr, source)
			store.delete(attr)
		} else {
			const restored = toRussian(source)
			if (restored !== null) writeAttr(el, attr, restored)
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
	selfText.clear()
	selfAttrs.clear()
	try {
		work()
	} finally {
		applying = false
	}
}

/** Starts the translator. Safe to call more than once. */
export function startAutoTranslate() {
	if (typeof document === "undefined" || observer) return

	observer = new MutationObserver((records) => {
		if (applying) return
		const english = i18n.current === "en"
		// Snapshot: runGuarded clears these before the work callback runs.
		const ownText = new Set(selfText)
		const ownAttrs = new Set(selfAttrs)
		runGuarded(() => {
			for (const record of records) {
				if (record.type === "characterData") {
					const node = record.target as Text
					// Our own write, already translated — anything else is Svelte
					// replacing the text, so the new value is the new original.
					if (ownText.has(node)) continue
					originalText.delete(node)
					applyText(node, english)
				} else if (record.type === "attributes") {
					const el = record.target as Element
					const attr = record.attributeName ?? ""
					if (ownAttrs.has(attrKey(el, attr))) continue
					originalAttrs.get(el)?.delete(attr)
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

	disposeEffect = $effect.root(() => {
		$effect(() => {
			const english = i18n.current === "en"
			runGuarded(() => walk(document.body, english))
		})
	})
}

/** Stops the translator and releases the observer and the language effect. */
export function stopAutoTranslate() {
	observer?.disconnect()
	observer = null
	disposeEffect?.()
	disposeEffect = null
	selfText.clear()
	selfAttrs.clear()
	selfKeyed = new WeakMap()
}
