import { describe, expect, it } from "vitest";
import { DEFAULT_SPLIT_BREAKPOINT, isSplitWidth, readSplitBreakpoint } from "./layoutMode";

describe("isSplitWidth", () => {
  it("is split at and above the breakpoint only", () => {
    expect(isSplitWidth(1099, 1100)).toBe(false);
    expect(isSplitWidth(1100, 1100)).toBe(true);
    expect(isSplitWidth(1600, 1100)).toBe(true);
  });
});

describe("readSplitBreakpoint", () => {
  it("falls back to the default without a theme variable", () => {
    expect(readSplitBreakpoint(document.body)).toBe(DEFAULT_SPLIT_BREAKPOINT);
  });

  it("reads the theme variable", () => {
    document.body.style.setProperty("--qk-split-breakpoint", "900px");
    expect(readSplitBreakpoint(document.body)).toBe(900);
    document.body.style.removeProperty("--qk-split-breakpoint");
  });
});
