/**
 * Tests for the Markdown renderer.
 *
 * This module is a sanitizer: Modrinth descriptions are attacker-controlled
 * text that ends up inside `{@html}`. The escaping tests below are therefore
 * not style checks — each one stands for a way the launcher UI could be taken
 * over by a project page, so a failure here is a security regression.
 */
import { describe, expect, it } from "vitest"

import { renderMarkdown } from "./markdown"

describe("renderMarkdown — escaping", () => {
	it("escapes raw HTML instead of emitting it", () => {
		const out = renderMarkdown("<script>alert(1)</script>")
		expect(out).not.toContain("<script>")
		expect(out).toContain("&lt;script&gt;")
	})

	it("escapes an img tag that would carry an onerror handler", () => {
		const out = renderMarkdown('<img src=x onerror="alert(1)">')
		expect(out).not.toContain("<img src=x")
		expect(out).toContain("&lt;img")
		expect(out).toContain("&quot;")
	})

	it("escapes single quotes", () => {
		// Nothing is exploitable while every attribute we emit uses double
		// quotes, but a sanitizer must not depend on that invariant.
		expect(renderMarkdown("it's")).toContain("&#39;")
	})

	it("escapes ampersands before adding any markup", () => {
		expect(renderMarkdown("a & b")).toContain("a &amp; b")
	})
})

describe("renderMarkdown — links", () => {
	it("renders an https link with target and rel", () => {
		const out = renderMarkdown("[label](https://example.com/page)")
		expect(out).toContain('href="https://example.com/page"')
		expect(out).toContain('target="_blank"')
		expect(out).toContain('rel="noopener noreferrer"')
		expect(out).toContain(">label</a>")
	})

	it("refuses a javascript: URL", () => {
		const out = renderMarkdown("[click](javascript:alert(1))")
		// The scheme survives only as inert paragraph text — what matters is
		// that no anchor (and therefore no navigable href) is produced.
		expect(out).not.toContain("<a")
		expect(out).not.toContain("href=")
	})

	it("refuses a protocol-relative URL", () => {
		// `//host/x` would load remote content just like https://host/x.
		const out = renderMarkdown("[click](//evil.example/x)")
		expect(out).not.toContain("<a")
	})

	it("refuses a data: URL", () => {
		const out = renderMarkdown("[click](data:text/html;base64,PHNjcmlwdD4=)")
		expect(out).not.toContain("<a")
	})

	it("cannot break out of the href attribute with a quote", () => {
		const out = renderMarkdown('[a](https://x/y"onmouseover=alert(1))')
		// The quote survives only as an entity, so the attribute stays closed
		// and no event handler is introduced.
		expect(out).not.toContain('"onmouseover')
		expect(out).toContain("&quot;")
	})
})

describe("renderMarkdown — images", () => {
	it("renders an https image lazily", () => {
		const out = renderMarkdown("![alt text](https://example.com/a.png)")
		expect(out).toContain('src="https://example.com/a.png"')
		expect(out).toContain('alt="alt text"')
		expect(out).toContain('loading="lazy"')
	})

	it("prefers the image rule over the link rule", () => {
		// Without image-before-link ordering the `!` is left behind and the
		// `[]()` part becomes an anchor.
		const out = renderMarkdown("![a](https://example.com/a.png)")
		expect(out).toContain("<img")
		expect(out).not.toContain("<a ")
	})

	it("refuses a javascript: image source", () => {
		const out = renderMarkdown("![a](javascript:alert(1))")
		expect(out).not.toContain("<img")
	})
})

describe("renderMarkdown — supported raw HTML", () => {
	it("renders Modrinth image links and centered blocks", () => {
		const out = renderMarkdown(
			'<center>\n<a href="https://modrinth.com/resourcepack/example">\n<img src="https://files.catbox.moe/example.png" width="700">\n</a>\n</center>',
		)
		expect(out).toContain('<div class="md-align-center">')
		expect(out).toContain('class="md-link" href="https://modrinth.com/resourcepack/example"')
		expect(out).toContain('src="https://files.catbox.moe/example.png"')
		expect(out).toContain('width="700"')
		expect(out).toContain("</div>")
		expect(out).not.toContain("&lt;center&gt;")
	})

	it("supports centered paragraph alignment", () => {
		const out = renderMarkdown('<p align="center">\n<img src="https://example.com/banner.png">\n</p>')
		expect(out).toContain('<div class="md-align-center">')
		expect(out).toContain('src="https://example.com/banner.png"')
	})

	it("renders the compact HTML layout used by Modrinth descriptions", () => {
		const out = renderMarkdown(
			'<center><a href="https://modrinth.com/resourcepack/example"><img src="https://files.catbox.moe/example.png" width="700"></a></center>',
		)
		expect(out).toContain('<div class="md-align-center">')
		expect(out).toContain('<a class="md-link"')
		expect(out).toContain('<img class="md-img"')
		expect(out).not.toContain('&lt;center&gt;')
	})

	it("keeps unsafe raw tags and attributes inert", () => {
		const out = renderMarkdown(
			'<script>alert(1)</script>\n<img src="https://example.com/a.png" onerror="alert(1)">\n<a href="javascript:alert(1)">bad</a>',
		)
		expect(out).not.toContain("<script>")
		expect(out).not.toContain('<img class="md-img"')
		expect(out).not.toContain('<a class="md-link"')
		expect(out).toContain("&lt;script&gt;")
	})
})

describe("renderMarkdown — code", () => {
	it("treats markdown inside a code span as literal text", () => {
		// Regression: the inline-code rule used to run after the link rule, so
		// a link written inside backticks became a real anchor.
		const out = renderMarkdown("use `[a](https://x)` verbatim")
		expect(out).toContain("<code>")
		expect(out).not.toContain("<a ")
		expect(out).toContain("[a](https://x)")
	})

	it("does not apply bold inside a code span", () => {
		const out = renderMarkdown("`**not bold**`")
		expect(out).not.toContain("<strong>")
	})

	it("escapes HTML inside a code span", () => {
		const out = renderMarkdown("`<b>x</b>`")
		expect(out).toContain("&lt;b&gt;")
		expect(out).not.toContain("<b>")
	})

	it("renders a fenced block without doubling newlines", () => {
		// Regression: fence lines were pushed with a trailing "\n" and the whole
		// document was then joined with "\n", so every line gained a blank one.
		const out = renderMarkdown("```\nline1\nline2\n```")
		expect(out).toContain("<pre><code>")
		expect(out).toContain("</code></pre>")
		expect(out).toContain("line1\nline2")
		expect(out).not.toContain("line1\n\nline2")
	})

	it("closes an unterminated fence", () => {
		const out = renderMarkdown("```\nstill open")
		expect(out).toContain("</code></pre>")
	})

	it("escapes HTML inside a fenced block", () => {
		const out = renderMarkdown("```\n<script>x</script>\n```")
		expect(out).not.toContain("<script>")
	})
})

describe("renderMarkdown — blocks", () => {
	it("shifts headings one level down", () => {
		// The details sheet already owns the h1/h2, so a document heading must
		// not compete with it.
		expect(renderMarkdown("# Title")).toContain("<h2>Title</h2>")
		expect(renderMarkdown("## Sub")).toContain("<h3>Sub</h3>")
	})

	it("clamps heading depth at h6", () => {
		expect(renderMarkdown("###### Deep")).toContain("<h6>Deep</h6>")
	})

	it("renders and closes an unordered list", () => {
		const out = renderMarkdown("- one\n- two")
		expect(out).toContain("<ul>")
		expect(out).toContain("</ul>")
		expect(out.match(/<li>/g)).toHaveLength(2)
	})

	it("renders an ordered list", () => {
		const out = renderMarkdown("1. one\n2. two")
		expect(out).toContain("<ol>")
		expect(out).toContain("</ol>")
	})

	it("closes a list when the block type changes", () => {
		const out = renderMarkdown("- item\n\nparagraph")
		expect(out.indexOf("</ul>")).toBeLessThan(out.indexOf("<p>paragraph</p>"))
	})

	it("renders a blockquote and a horizontal rule", () => {
		expect(renderMarkdown("> quoted")).toContain("<blockquote>quoted</blockquote>")
		expect(renderMarkdown("---")).toContain("<hr />")
	})

	it("renders emphasis", () => {
		expect(renderMarkdown("**bold**")).toContain("<strong>bold</strong>")
		expect(renderMarkdown("*italic*")).toContain("<em>italic</em>")
	})

	it("normalises CRLF input", () => {
		const out = renderMarkdown("# A\r\n\r\ntext")
		expect(out).toContain("<h2>A</h2>")
		expect(out).not.toContain("\r")
	})

	it("returns an empty string for empty input", () => {
		expect(renderMarkdown("")).toBe("")
		expect(renderMarkdown("\n\n  \n")).toBe("")
	})
})
