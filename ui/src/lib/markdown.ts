import DOMPurify from "dompurify";
import { Marked } from "marked";

const parser = new Marked({ gfm: true, breaks: false, async: false });

/** Markdown (release notes) as sanitized HTML: no scripts, styles, images or inline handlers. */
export function renderMarkdown(source: string): string {
  const html = parser.parse(source, { async: false });
  return DOMPurify.sanitize(html, {
    FORBID_TAGS: ["style", "img", "iframe", "form", "input"],
    FORBID_ATTR: ["style"],
  });
}
