import type { Category } from "../bindings/Category";
import type { Game } from "../bindings/Game";
import type { GroupBy } from "../bindings/GroupBy";
import { storesOf } from "./copies";
import type { NavKey } from "./gridNav";
import { NO_CATEGORY } from "./views";

/** One group of a grouped view. A game with several categories is in each of their groups. */
export interface GameGroup {
  /** Category id, store id, `NO_CATEGORY`, or "" for the single group of an ungrouped view. */
  key: string;
  label: string;
  /** The store, when grouped by store (for its icon). */
  source: string | null;
  games: Game[];
}

const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

/**
 * Splits the (already sorted) games into groups, keeping their order inside each group. Category
 * groups follow the categories' order with "No category" last; store groups are ordered by name.
 * Without grouping there is one group, key "".
 */
export function groupGames(
  games: readonly Game[],
  by: GroupBy,
  categories: readonly Category[],
  sourceName: (id: string) => string,
): GameGroup[] {
  if (by === "none") return [{ key: "", label: "", source: null, games: [...games] }];
  const groups = new Map<string, GameGroup>();
  const add = (key: string, label: string, source: string | null, game: Game) => {
    let group = groups.get(key);
    if (!group) {
      group = { key, label, source, games: [] };
      groups.set(key, group);
    }
    group.games.push(game);
  };
  if (by === "source") {
    // A game in several stores shows in each of them.
    for (const game of games) {
      for (const source of storesOf(game)) add(source, sourceName(source), source, game);
    }
  } else {
    const names = new Map(categories.map((c) => [c.id, c.name]));
    for (const game of games) {
      const known = game.categories.filter((id) => names.has(id));
      if (known.length === 0) add(NO_CATEGORY, "No category", null, game);
      for (const id of known) add(id, names.get(id) ?? "", null, game);
    }
  }
  if (by === "source") {
    return [...groups.values()].toSorted((a, b) => collator.compare(a.label, b.label));
  }
  const order = new Map(categories.map((c, index) => [c.id, index]));
  const rank = (group: GameGroup) => order.get(group.key) ?? categories.length;
  return [...groups.values()].toSorted((a, b) => rank(a) - rank(b));
}

/** One game's place in a grouped layout (the same game can have several). */
export interface Slot {
  game: Game;
  groupKey: string;
  /** Index of its row in `GroupedLayout.rows`. */
  row: number;
  column: number;
}

export type LayoutRow =
  | { kind: "header"; group: GameGroup; collapsed: boolean; top: number; height: number }
  | { kind: "games"; groupKey: string; first: number; count: number; top: number; height: number };

export type HeaderRow = Extract<LayoutRow, { kind: "header" }>;
export type GamesRow = Extract<LayoutRow, { kind: "games" }>;

export const asHeaderRow = (row: LayoutRow | undefined): HeaderRow | undefined =>
  row?.kind === "header" ? row : undefined;
export const asGamesRow = (row: LayoutRow | undefined): GamesRow | undefined =>
  row?.kind === "games" ? row : undefined;

/** Slot indices of a games row. */
export function rowSlots(row: GamesRow): number[] {
  return Array.from({ length: row.count }, (_, i) => row.first + i);
}

export interface GroupedLayout {
  rows: LayoutRow[];
  slots: Slot[];
  /** Full content height, padding included. */
  height: number;
}

export interface LayoutOptions {
  columns: number;
  rowHeight: number;
  headerHeight: number;
  /** Space above the first and below the last row. */
  padding: number;
  /** Whether groups get header rows (false for an ungrouped view). */
  headers: boolean;
  /** Keys of collapsed groups: only their header shows. */
  collapsed: ReadonlySet<string>;
}

/** Rows (headers and rows of up to `columns` games) with their offsets, and every game slot. */
export function layoutGroups(groups: readonly GameGroup[], options: LayoutOptions): GroupedLayout {
  const columns = Math.max(1, Math.floor(options.columns));
  const rows: LayoutRow[] = [];
  const slots: Slot[] = [];
  let top = options.padding;
  for (const group of groups) {
    const collapsed = options.headers && options.collapsed.has(group.key);
    if (options.headers) {
      rows.push({ kind: "header", group, collapsed, top, height: options.headerHeight });
      top += options.headerHeight;
    }
    if (collapsed) continue;
    for (let start = 0; start < group.games.length; start += columns) {
      const chunk = group.games.slice(start, start + columns);
      const row = rows.length;
      rows.push({
        kind: "games",
        groupKey: group.key,
        first: slots.length,
        count: chunk.length,
        top,
        height: options.rowHeight,
      });
      chunk.forEach((game, column) => slots.push({ game, groupKey: group.key, row, column }));
      top += options.rowHeight;
    }
  }
  return { rows, slots, height: rows.length > 0 ? top + options.padding : 0 };
}

/** Rows intersecting the viewport, extended by `overscan` rows on both sides. */
export function visibleRowRange(
  layout: GroupedLayout,
  scrollTop: number,
  viewportHeight: number,
  overscan: number,
): { start: number; end: number } {
  const { rows } = layout;
  if (rows.length === 0) return { start: 0, end: 0 };
  // First row whose bottom is below the viewport's top.
  let low = 0;
  let high = rows.length - 1;
  while (low < high) {
    const mid = (low + high) >> 1;
    const row = rows[mid];
    if (row && row.top + row.height <= scrollTop) low = mid + 1;
    else high = mid;
  }
  const bottom = scrollTop + Math.max(0, viewportHeight);
  let last = low;
  while (last + 1 < rows.length && (rows[last + 1]?.top ?? Infinity) < bottom) last++;
  return {
    start: Math.max(0, low - overscan),
    end: Math.min(rows.length, last + 1 + overscan),
  };
}

/**
 * Smallest scroll change that shows a row completely; scrolling up also shows its group header
 * when the header is right above it. Returns `scrollTop` when the row is already visible.
 */
export function scrollTopForRow(
  layout: GroupedLayout,
  index: number,
  scrollTop: number,
  viewportHeight: number,
  padding: number,
): number {
  const row = layout.rows[index];
  if (!row) return scrollTop;
  const above = layout.rows[index - 1];
  const top = above?.kind === "header" ? above.top : row.top;
  const bottom = row.top + row.height;
  if (top < scrollTop) return Math.max(0, top - (index <= 1 ? padding : 0));
  if (bottom > scrollTop + viewportHeight) return bottom - viewportHeight + padding;
  return scrollTop;
}

/** The next game row after (`step` 1) or before (-1) `row`, skipping headers. */
function gamesRow(layout: GroupedLayout, row: number, step: 1 | -1): number | null {
  for (let i = row + step; i >= 0 && i < layout.rows.length; i += step) {
    if (layout.rows[i]?.kind === "games") return i;
  }
  return null;
}

function slotIn(layout: GroupedLayout, row: number, column: number): number {
  const target = layout.rows[row];
  if (target?.kind !== "games") return -1;
  return target.first + Math.min(column, target.count - 1);
}

/**
 * The slot to select after `key`, moving through the rows as shown: left/right step through all
 * slots (across rows and groups), up/down go to the row above/below (skipping headers) in the
 * same column or the row's last game, PageUp/PageDown move `pageRows` rows, Home/End jump to the
 * first/last game. `-1` only when there are no slots; a `current` of -1 starts at the first.
 */
export function navigateSlots(
  layout: GroupedLayout,
  current: number,
  key: NavKey,
  pageRows: number,
): number {
  const count = layout.slots.length;
  if (count === 0) return -1;
  const last = count - 1;
  const slot = layout.slots[current];
  if (!slot) return key === "End" ? last : 0;
  switch (key) {
    case "ArrowLeft":
      return Math.max(0, current - 1);
    case "ArrowRight":
      return Math.min(last, current + 1);
    case "Home":
      return 0;
    case "End":
      return last;
    case "ArrowUp":
    case "ArrowDown": {
      const row = gamesRow(layout, slot.row, key === "ArrowUp" ? -1 : 1);
      return row === null ? current : slotIn(layout, row, slot.column);
    }
    case "PageUp":
    case "PageDown": {
      const step = key === "PageUp" ? -1 : 1;
      let row = slot.row;
      for (let i = 0; i < Math.max(1, Math.floor(pageRows)); i++) {
        const next = gamesRow(layout, row, step);
        if (next === null) break;
        row = next;
      }
      if (row === slot.row) return step < 0 ? 0 : last;
      return slotIn(layout, row, slot.column);
    }
  }
}

/** DOM id of a game's element; with groups the same game can show several times. */
export function slotKey(slot: Pick<Slot, "game" | "groupKey">): string {
  return slot.groupKey === "" ? slot.game.id : `${slot.game.id}:${slot.groupKey}`;
}

/**
 * The group header to pin at the top of a grouped view scrolled to `scrollTop`: the last header
 * at or above the top edge. `offset` (zero or negative) pushes it up while the next header
 * scrolls in under it. `null` while the first header is still below the top, or without groups.
 */
export function stickyHeader(
  layout: GroupedLayout,
  scrollTop: number,
): { row: HeaderRow; offset: number } | null {
  let current: HeaderRow | undefined;
  for (const row of layout.rows) {
    if (row.kind !== "header") continue;
    if (row.top > scrollTop) {
      if (!current) return null;
      return { row: current, offset: Math.min(0, row.top - scrollTop - current.height) };
    }
    current = row;
  }
  return current ? { row: current, offset: 0 } : null;
}
