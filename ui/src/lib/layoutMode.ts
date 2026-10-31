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

/** Reads a pixel value from a theme variable, falling back when it is missing or not a number. */
export function readCssPx(element: Element, name: string, fallback: number): number {
  const value = Number.parseFloat(getComputedStyle(element).getPropertyValue(name));
  return Number.isFinite(value) && value > 0 ? value : fallback;
}
