/** Fallback for `--qk-split-breakpoint` (theme.css) when the variable cannot be read. */
export const DEFAULT_SPLIT_BREAKPOINT = 1100;

/** True when the content area is wide enough to show details beside the library. */
export function isSplitWidth(width: number, breakpoint: number): boolean {
  return width >= breakpoint;
}

/** Reads the split breakpoint (px) from the theme, falling back to the default. */
export function readSplitBreakpoint(element: Element): number {
  const raw = getComputedStyle(element).getPropertyValue("--qk-split-breakpoint");
  const value = Number.parseFloat(raw);
  return Number.isFinite(value) && value > 0 ? value : DEFAULT_SPLIT_BREAKPOINT;
}
