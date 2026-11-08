import { describe, expect, it } from "vitest";
import { activeSection, type SectionBox } from "./scrollSpy";

const boxes = (...tops: number[]): SectionBox[] => tops.map((top, i) => ({ id: `s${i}`, top }));

describe("activeSection", () => {
  it("marks the section crossing the middle of the view", () => {
    // View 800 px high: the middle is at 400.
    expect(activeSection(boxes(0, 300, 900), 800, false)).toBe("s1");
    expect(activeSection(boxes(-600, 100, 500), 800, false)).toBe("s1");
  });

  it("marks the first section before any reaches the middle", () => {
    expect(activeSection(boxes(450, 900), 800, false)).toBe("s0");
  });

  it("marks the section above when the middle is in a gap", () => {
    expect(activeSection(boxes(0, 390, 410), 800, false)).toBe("s1");
  });

  it("marks the last section at the end of the page", () => {
    expect(activeSection(boxes(-900, 100, 600), 800, true)).toBe("s2");
  });

  it("is null without sections", () => {
    expect(activeSection([], 800, false)).toBeNull();
  });
});
