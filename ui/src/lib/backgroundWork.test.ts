import { describe, expect, it } from "vitest";
import { IDLE_WORK, isWorkBusy, workLabel } from "./backgroundWork";

describe("background work", () => {
  it("is idle without queued jobs", () => {
    expect(isWorkBusy(IDLE_WORK)).toBe(false);
    expect(workLabel(IDLE_WORK)).toBeNull();
  });

  it("names what is downloading", () => {
    expect(workLabel({ covers: 12, details: 0 })).toBe("Downloading 12 covers…");
    expect(workLabel({ covers: 0, details: 1 })).toBe("Downloading details of 1 game…");
    expect(workLabel({ covers: 1, details: 87 })).toBe(
      "Downloading 1 cover and details of 87 games…",
    );
    expect(isWorkBusy({ covers: 0, details: 3 })).toBe(true);
  });
});
