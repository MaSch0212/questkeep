/** Where a section starts, relative to the top of the visible scroll area (px). */
export interface SectionBox {
  id: string;
  top: number;
}

/** The visible scroll area. */
export interface ScrollView {
  /** Its height (px). */
  height: number;
  /** How far it can still scroll down (px); 0 at the end. */
  remaining: number;
  /** How far below the top edge a section may start and already count (px). */
  offset?: number;
}

/** A section counts from a little before its heading reaches the top edge. */
export const SECTION_OFFSET = 24;

/**
 * The section to mark in a section menu: the last one that starts at or above a line just below
 * the top of the visible area, so a section scrolled to the top is the marked one however tall
 * it is. Before any section reaches the line, the first one.
 *
 * Sections near the end can never reach the top. So over the last screen of scrolling the line
 * moves down with the scroll position and is at the bottom edge at the very end: every section
 * gets its turn and the last one is marked at the end. Null without sections.
 */
export function activeSection(sections: readonly SectionBox[], view: ScrollView): string | null {
  if (sections.length === 0) return null;
  const height = Math.max(0, view.height);
  const offset = Math.min(Math.max(0, view.offset ?? SECTION_OFFSET), height);
  // Layout rounding can leave a pixel or two at the end.
  const remaining = view.remaining <= 2 ? 0 : view.remaining;
  const line = Math.max(offset, height - remaining);
  let active = sections[0]?.id ?? null;
  for (const section of sections) {
    if (section.top > line) break;
    active = section.id;
  }
  return active;
}
