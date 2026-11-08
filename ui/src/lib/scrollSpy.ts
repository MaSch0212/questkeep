/** Where a section starts, relative to the top of the visible scroll area (px). */
export interface SectionBox {
  id: string;
  top: number;
}

/**
 * The section to mark in a section menu: the one crossing the middle of the visible area (in a
 * gap between two, the one above). Scrolled to the end, the last section, which may be too short
 * to ever reach the middle. Null without sections.
 */
export function activeSection(
  sections: readonly SectionBox[],
  viewHeight: number,
  atEnd: boolean,
): string | null {
  if (sections.length === 0) return null;
  if (atEnd) return sections.at(-1)?.id ?? null;
  const middle = viewHeight / 2;
  let active = sections[0]?.id ?? null;
  for (const section of sections) {
    if (section.top > middle) break;
    active = section.id;
  }
  return active;
}
