import { describe, expect, it } from "vitest";
import { TOOLTIP_DELAY_MS, TOOLTIP_WARM_MS, tooltipDelay } from "./tooltip";

describe("tooltipDelay", () => {
  it("waits on the first tooltip and shows at once while warm", () => {
    expect(tooltipDelay(1000, null, false)).toBe(TOOLTIP_DELAY_MS);
    expect(tooltipDelay(1000, null, true)).toBe(0);
    expect(tooltipDelay(1000, 1000 - TOOLTIP_WARM_MS + 1, false)).toBe(0);
    expect(tooltipDelay(1000, 1000 - TOOLTIP_WARM_MS, false)).toBe(TOOLTIP_DELAY_MS);
  });
});
