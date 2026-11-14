import { describe, expect, it } from "vitest";
import { activeSection, type SectionBox } from "./scrollSpy";

const boxes = (...tops: number[]): SectionBox[] => tops.map((top, i) => ({ id: `s${i}`, top }));
/** A view 800 px high with plenty left to scroll. */
const far = { height: 800, remaining: 5000 };

describe("activeSection", () => {
  it("marks the section at the top of the view", () => {
    expect(activeSection(boxes(-600, 0, 900), far)).toBe("s1");
    expect(activeSection(boxes(-600, -100, 500), far)).toBe("s1");
  });

  it("keeps a tall section marked when the next one is already in view", () => {
    // Scrolled to the second section's top; the third starts above the middle of the view.
    expect(activeSection(boxes(-400, 0, 300, 1200), far)).toBe("s1");
    // A short section at the top stays marked too, although the next one fills the view.
    expect(activeSection(boxes(-400, 0, 120), far)).toBe("s1");
  });

  it("marks a section from a little before it reaches the top", () => {
    expect(activeSection(boxes(-600, 20, 900), far)).toBe("s1");
    expect(activeSection(boxes(-600, 40, 900), far)).toBe("s0");
    expect(activeSection(boxes(-600, 40, 900), { ...far, offset: 48 })).toBe("s1");
  });

  it("marks the first section before any reaches the top", () => {
    expect(activeSection(boxes(450, 900), far)).toBe("s0");
  });

  it("marks the last section at the end of the page", () => {
    expect(activeSection(boxes(-900, 100, 600), { height: 800, remaining: 0 })).toBe("s2");
    expect(activeSection(boxes(-900, 100, 600), { height: 800, remaining: 1.5 })).toBe("s2");
  });

  it("gives every section near the end its turn over the last screen of scrolling", () => {
    // Three short sections at the end; none can reach the top. 500 px left: the line is at 300.
    expect(activeSection(boxes(-900, 250, 500, 700), { height: 800, remaining: 500 })).toBe("s1");
    // 200 px left: the same sections are 300 px higher, the line is at 600.
    expect(activeSection(boxes(-1200, -50, 200, 400), { height: 800, remaining: 200 })).toBe("s3");
    expect(activeSection(boxes(-1100, 50, 300, 650), { height: 800, remaining: 300 })).toBe("s2");
  });

  it("copes with a view without height and is null without sections", () => {
    expect(activeSection(boxes(-10, 5), { height: 0, remaining: 0 })).toBe("s0");
    expect(activeSection([], far)).toBeNull();
  });
});
