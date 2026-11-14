import { describe, expect, it } from "vitest";
import { renderMarkdown } from "./markdown";

describe("renderMarkdown", () => {
  it("renders headings, lists and inline code", () => {
    const html = renderMarkdown("### Minor changes\n\n- One `code`\n- Two **bold**\n");
    expect(html).toContain("<h3>Minor changes</h3>");
    expect(html).toContain("<li>One <code>code</code></li>");
    expect(html).toContain("<strong>bold</strong>");
  });

  it("drops scripts, handlers and images", () => {
    const html = renderMarkdown(
      '<script>alert(1)</script><a href="https://x.test" onclick="evil()">x</a>![i](https://x.test/i.png)',
    );
    expect(html).not.toContain("<script");
    expect(html).not.toContain("onclick");
    expect(html).not.toContain("<img");
    expect(html).toContain('href="https://x.test"');
  });
});
