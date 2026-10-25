import { describe, expect, it } from "vitest";
import { titleWhenCut } from "./truncation";

function span(scrollWidth: number, clientWidth: number): HTMLElement {
  const el = document.createElement("span");
  el.textContent = "Enter the Gungeon";
  Object.defineProperty(el, "scrollWidth", { value: scrollWidth });
  Object.defineProperty(el, "clientWidth", { value: clientWidth });
  return el;
}

describe("titleWhenCut", () => {
  it("adds the text as a tooltip only when it doesn't fit", () => {
    const cut = span(200, 80);
    titleWhenCut(cut);
    expect(cut.title).toBe("Enter the Gungeon");
    const fits = span(80, 80);
    fits.title = "stale";
    titleWhenCut(fits);
    expect(fits.hasAttribute("title")).toBe(false);
  });
});
