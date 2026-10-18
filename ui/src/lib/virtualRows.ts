/** Fixed-height row virtualization math for the cover grid. */

export interface RowWindow {
  /** First row to render (inclusive). */
  start: number;
  /** Row after the last one to render (exclusive). */
  end: number;
}

export interface RowLayout {
  rowHeight: number;
  rowCount: number;
  /** Space above the first row. */
  paddingTop: number;
}

/** Rows intersecting the viewport, extended by `overscan` rows on both sides. */
export function visibleRows(
  scrollTop: number,
  viewportHeight: number,
  layout: RowLayout,
  overscan = 3,
): RowWindow {
  const { rowHeight, rowCount, paddingTop } = layout;
  if (rowCount <= 0 || rowHeight <= 0) return { start: 0, end: 0 };
  const top = Math.max(0, scrollTop - paddingTop);
  const first = Math.floor(top / rowHeight);
  const last = Math.ceil((top + Math.max(0, viewportHeight)) / rowHeight);
  return {
    start: Math.max(0, Math.min(rowCount - 1, first - overscan)),
    end: Math.min(rowCount, last + overscan),
  };
}

/** Top offset of a row inside the scroll content. */
export function rowTop(row: number, layout: RowLayout): number {
  return layout.paddingTop + row * layout.rowHeight;
}

/**
 * Smallest scroll change that makes `row` fully visible; returns `scrollTop` unchanged when the
 * row is already visible.
 */
export function scrollTopForRow(
  row: number,
  scrollTop: number,
  viewportHeight: number,
  layout: RowLayout,
): number {
  const top = rowTop(row, layout);
  const bottom = top + layout.rowHeight;
  if (top < scrollTop) return Math.max(0, top - layout.paddingTop);
  if (bottom > scrollTop + viewportHeight) return bottom - viewportHeight + layout.paddingTop;
  return scrollTop;
}

/** Row indices of a window, for rendering. */
export function rowRange({ start, end }: RowWindow): number[] {
  const rows: number[] = [];
  for (let i = start; i < end; i++) rows.push(i);
  return rows;
}
