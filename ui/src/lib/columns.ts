import type { Game } from "../bindings/Game";
import type { SortField } from "../bindings/SortField";
import type { ViewColumn } from "../bindings/ViewColumn";
import {
  formatDate,
  formatPlaytime,
  formatRelativeDate,
  formatReleaseDate,
  formatSize,
  releaseYear,
} from "./format";

/** Names the table needs that live outside the game (completion statuses, categories). */
export interface ColumnNames {
  status: (id: string) => string | undefined;
  category: (id: string) => string | undefined;
}

/** Default width of a column: `--qk-column-<size>` in theme.css. */
export type ColumnSize = "sm" | "md" | "lg";

export interface ColumnDef {
  id: string;
  label: string;
  size: ColumnSize;
  /** The sort a click on the header sets; none: the header doesn't sort. */
  sort?: SortField;
  /** Numbers line up at the end of the cell. */
  numeric?: boolean;
  /** The cell's text; columns without it (source, install state) are drawn by the row. */
  text?: (game: Game, names: ColumnNames) => string;
}

/** Shown for a missing value. */
export const EMPTY = "–";

const list = (items: readonly string[]) => (items.length === 0 ? EMPTY : items.join(", "));
const score = (value: number | null | undefined) =>
  value === null || value === undefined ? EMPTY : String(Math.round(value));
const hours = (seconds: number | null | undefined) => (seconds ? formatPlaytime(seconds) : EMPTY);

/** Every column the table can show, in the picker's order for hidden ones (by label). */
export const COLUMNS: readonly ColumnDef[] = [
  { id: "source", label: "Source", size: "md" },
  { id: "state", label: "Install state", size: "md" },
  {
    id: "playtime",
    label: "Playtime",
    size: "md",
    sort: "playtime",
    text: (g) => formatPlaytime(g.playtimeSeconds),
  },
  {
    id: "lastPlayed",
    label: "Last played",
    size: "md",
    sort: "lastPlayed",
    text: (g) => formatRelativeDate(g.lastPlayed),
  },
  {
    id: "releaseYear",
    label: "Release year",
    size: "sm",
    sort: "releaseDate",
    numeric: true,
    text: (g) => releaseYear(g.releaseDate) ?? EMPTY,
  },
  {
    id: "releaseDate",
    label: "Release date",
    size: "md",
    sort: "releaseDate",
    text: (g) => formatReleaseDate(g.releaseDate) ?? EMPTY,
  },
  {
    id: "dateAdded",
    label: "Date added",
    size: "md",
    sort: "dateAdded",
    text: (g) => formatDate(g.dateAdded),
  },
  {
    id: "playCount",
    label: "Play count",
    size: "sm",
    sort: "playCount",
    numeric: true,
    text: (g) => String(g.playCount),
  },
  {
    id: "completionStatus",
    label: "Completion status",
    size: "md",
    sort: "completionStatus",
    text: (g, n) => (g.completionStatus ? (n.status(g.completionStatus) ?? EMPTY) : EMPTY),
  },
  {
    id: "userScore",
    label: "Your score",
    size: "sm",
    sort: "userScore",
    numeric: true,
    text: (g) => score(g.userScore),
  },
  {
    id: "criticScore",
    label: "Critic score",
    size: "sm",
    sort: "criticScore",
    numeric: true,
    text: (g) => score(g.criticScore?.value),
  },
  {
    id: "communityScore",
    label: "Community score",
    size: "sm",
    sort: "communityScore",
    numeric: true,
    text: (g) => score(g.communityScore?.value),
  },
  {
    id: "timeToBeat",
    label: "Time to beat",
    size: "sm",
    sort: "timeToBeat",
    numeric: true,
    text: (g) => hours(g.timeToBeat?.main),
  },
  {
    id: "installSize",
    label: "Install size",
    size: "sm",
    sort: "installSize",
    numeric: true,
    text: (g) => (g.installSize ? formatSize(g.installSize) : EMPTY),
  },
  { id: "favorite", label: "Favorite", size: "sm", text: (g) => (g.favorite ? "Yes" : EMPTY) },
  {
    id: "categories",
    label: "Categories",
    size: "lg",
    text: (g, n) => list(g.categories.map((id) => n.category(id) ?? id)),
  },
  {
    id: "developers",
    label: "Developers",
    size: "lg",
    sort: "developer",
    text: (g) => list(g.developers),
  },
  {
    id: "publishers",
    label: "Publishers",
    size: "lg",
    sort: "publisher",
    text: (g) => list(g.publishers),
  },
  { id: "series", label: "Series", size: "lg", sort: "series", text: (g) => list(g.series) },
  { id: "genres", label: "Genres", size: "lg", text: (g) => list(g.genres) },
  { id: "tags", label: "Tags", size: "lg", text: (g) => list(g.tags) },
  { id: "features", label: "Features", size: "lg", text: (g) => list(g.features) },
  { id: "platforms", label: "Platforms", size: "md", text: (g) => list(g.platforms) },
  { id: "gameModes", label: "Game modes", size: "lg", text: (g) => list(g.gameModes) },
  { id: "themes", label: "Themes", size: "lg", text: (g) => list(g.themes) },
  { id: "perspectives", label: "Perspectives", size: "md", text: (g) => list(g.perspectives) },
  { id: "ageRatings", label: "Age ratings", size: "md", text: (g) => list(g.ageRatings) },
  { id: "installDir", label: "Install folder", size: "lg", text: (g) => g.installDir ?? EMPTY },
];

const BY_ID = new Map(COLUMNS.map((c) => [c.id, c]));

/** The table's columns before anyone changes them (Name always comes first and isn't listed). */
export const DEFAULT_COLUMNS: readonly ViewColumn[] = [
  "source",
  "state",
  "playtime",
  "lastPlayed",
  "releaseYear",
].map((id) => ({ id, width: null }));

export interface ShownColumn {
  def: ColumnDef;
  /** Width in px, or null for the column's default width. */
  width: number | null;
}

/** A view's columns as stored: empty means the defaults. */
function effective(columns: readonly ViewColumn[]): readonly ViewColumn[] {
  return columns.length === 0 ? DEFAULT_COLUMNS : columns;
}

/** The columns a view shows, in order; unknown or repeated ids are skipped. */
export function shownColumns(columns: readonly ViewColumn[]): ShownColumn[] {
  const seen = new Set<string>();
  return effective(columns).flatMap((column) => {
    const def = BY_ID.get(column.id);
    if (!def || seen.has(column.id)) return [];
    seen.add(column.id);
    return [{ def, width: column.width }];
  });
}

/** Stored form of a column list: the defaults (unchanged) are stored as an empty list. */
function normalized(columns: ViewColumn[]): ViewColumn[] {
  return sameColumns(columns, DEFAULT_COLUMNS) ? [] : columns;
}

export function sameColumns(a: readonly ViewColumn[], b: readonly ViewColumn[]): boolean {
  const x = effective(a);
  const y = effective(b);
  return (
    x.length === y.length &&
    x.every((c, i) => c.id === y[i]?.id && (c.width ?? null) === (y[i]?.width ?? null))
  );
}

/** Shows a hidden column at the end, or hides a shown one. */
export function toggleColumn(columns: readonly ViewColumn[], id: string): ViewColumn[] {
  const current = [...effective(columns)];
  return normalized(
    current.some((c) => c.id === id)
      ? current.filter((c) => c.id !== id)
      : [...current, { id, width: null }],
  );
}

/** Moves column `id` to the place of column `to`. */
export function moveColumn(columns: readonly ViewColumn[], id: string, to: string): ViewColumn[] {
  const current = [...effective(columns)];
  const from = current.findIndex((c) => c.id === id);
  const target = current.findIndex((c) => c.id === to);
  if (from < 0 || target < 0 || from === target) return [...columns];
  const [moved] = current.splice(from, 1);
  if (moved) current.splice(target, 0, moved);
  return normalized(current);
}

/** Sets a column's width in whole px (at least `min`). */
export function resizeColumn(
  columns: readonly ViewColumn[],
  id: string,
  width: number,
  min: number,
): ViewColumn[] {
  const px = Math.max(min, Math.round(width));
  return normalized(effective(columns).map((c) => (c.id === id ? { id: c.id, width: px } : c)));
}

/**
 * The CSS grid columns of the table: icon, Name (takes the rest), then each column. Columns at
 * their default width give way (down to `--qk-column-min`) before the table scrolls sideways.
 */
export function columnTemplate(columns: readonly ShownColumn[]): string {
  return [
    "var(--qk-list-thumb-width)",
    "minmax(var(--qk-column-name-min), 1fr)",
    ...columns.map((c) =>
      c.width === null
        ? `minmax(var(--qk-column-min), var(--qk-column-${c.def.size}))`
        : `${c.width}px`,
    ),
  ].join(" ");
}

/** The picker's entries: shown columns in their order, then the others by label. */
export function pickerColumns(
  columns: readonly ViewColumn[],
): { def: ColumnDef; shown: boolean }[] {
  const shown = shownColumns(columns).map((c) => ({ def: c.def, shown: true }));
  const ids = new Set(shown.map((c) => c.def.id));
  const hidden = COLUMNS.filter((c) => !ids.has(c.id))
    .toSorted((a, b) => a.label.localeCompare(b.label))
    .map((def) => ({ def, shown: false }));
  return [...shown, ...hidden];
}

/** The table's narrowest width (CSS, without the scroll bar): below it the table scrolls sideways. */
export function columnsMinWidth(columns: readonly ShownColumn[]): string {
  const widths = columns.map((c) => (c.width === null ? "var(--qk-column-min)" : `${c.width}px`));
  return `calc(${[
    "var(--qk-list-thumb-width)",
    "var(--qk-column-name-min)",
    ...widths,
    `${columns.length + 1} * var(--qk-list-gap)`,
    "2 * var(--qk-list-inline-pad)",
  ].join(" + ")})`;
}
