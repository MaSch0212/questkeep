import { readFileSync } from "node:fs";
import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { ScriptList } from "./ScriptList";

describe("ScriptList", () => {
  // Screen-reader-only texts are absolutely positioned. Outside a positioned box they sit at their
  // place in the settings page, far below the window, and made the whole window scroll along.
  it("keeps its screen-reader texts inside a positioned box", () => {
    const { container } = render(() => (
      <ScriptList
        label="Pre scripts"
        rows={[{ key: "slot", kind: "placeholder", title: "Game's own", hint: "" }]}
        reorderable
        onMove={() => undefined}
        onNudge={() => undefined}
        row={() => <span>row</span>}
      />
    ));
    const box = container.querySelector(".script-list-box");
    expect(box).not.toBeNull();
    const hidden = container.querySelectorAll(".visually-hidden");
    expect(hidden.length).toBeGreaterThan(0);
    for (const element of hidden) expect(box?.contains(element)).toBe(true);

    // Tests run from ui/.
    const css = readFileSync("src/app.css", "utf8");
    expect(css).toMatch(/\.script-list-box\s*\{[^}]*position:\s*relative/);
  });
});
