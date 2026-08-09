/**
 * Minimal Markdown renderer for Modrinth project descriptions.
 *
 * Modrinth returns the long description (`body`) as Markdown that often
 * contains raw HTML. Everything is escaped first and only the small set of
 * tags the details view needs is re-introduced afterwards, so a project page
 * can never inject markup or scripts into the launcher.
 *
 * Links become real anchors that carry the target URL, but the WebView never
 * follows them: the details view intercepts the click and hands the URL to the
 * system browser, because navigating the WebView would replace the whole
 * launcher UI with the website.
 */

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Inline formatting inside one already block-classified line. */
function inline(text: string): string {
  let out = escapeHtml(text);
  // Images before links, otherwise the link rule eats the `![]()` syntax.
  out = out.replace(
    /!\[([^\]]*)\]\((https?:\/\/[^\s)]+)\)/g,
    (_match, alt: string, url: string) =>
      `<img class="md-img" src="${url}" alt="${alt}" loading="lazy" />`,
  );
  // The URL went through escapeHtml above, so a quote in it cannot break out
  // of the attribute.
  out = out.replace(
    /\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,
    (_match, label: string, url: string) =>
      `<a class="md-link" href="${url}">${label}</a>`,
  );
  out = out.replace(/`([^`]+)`/g, "<code>$1</code>");
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  out = out.replace(/(^|[^*])\*([^*]+)\*/g, "$1<em>$2</em>");
  return out;
}

/** Renders a Markdown document to a safe HTML string. */
export function renderMarkdown(source: string): string {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const html: string[] = [];
  let list: "ul" | "ol" | null = null;
  let inCode = false;

  const closeList = () => {
    if (list) {
      html.push(`</${list}>`);
      list = null;
    }
  };

  for (const raw of lines) {
    const line = raw.trimEnd();

    if (line.trim().startsWith("```")) {
      if (inCode) {
        html.push("</code></pre>");
        inCode = false;
      } else {
        closeList();
        html.push("<pre><code>");
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      html.push(`${escapeHtml(raw)}\n`);
      continue;
    }

    if (!line.trim()) {
      closeList();
      continue;
    }

    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      closeList();
      // Shift one level down: the sheet already has its own title heading.
      const level = Math.min((heading[1] ?? "#").length + 1, 6);
      html.push(`<h${level}>${inline(heading[2] ?? "")}</h${level}>`);
      continue;
    }

    if (/^(-{3,}|\*{3,}|_{3,})$/.test(line.trim())) {
      closeList();
      html.push("<hr />");
      continue;
    }

    const bullet = /^\s*[-*+]\s+(.*)$/.exec(line);
    if (bullet) {
      if (list !== "ul") {
        closeList();
        html.push("<ul>");
        list = "ul";
      }
      html.push(`<li>${inline(bullet[1] ?? "")}</li>`);
      continue;
    }

    const numbered = /^\s*\d+[.)]\s+(.*)$/.exec(line);
    if (numbered) {
      if (list !== "ol") {
        closeList();
        html.push("<ol>");
        list = "ol";
      }
      html.push(`<li>${inline(numbered[1] ?? "")}</li>`);
      continue;
    }

    const quote = /^>\s?(.*)$/.exec(line);
    if (quote) {
      closeList();
      html.push(`<blockquote>${inline(quote[1] ?? "")}</blockquote>`);
      continue;
    }

    closeList();
    html.push(`<p>${inline(line)}</p>`);
  }

  if (inCode) html.push("</code></pre>");
  closeList();
  return html.join("\n");
}
