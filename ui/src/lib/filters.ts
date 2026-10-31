import type { FilterGroup } from "../bindings/FilterGroup";
import type { FilterNode } from "../bindings/FilterNode";
import type { Game } from "../bindings/Game";
import type { ListField } from "../bindings/ListField";
import type { ListFilter } from "../bindings/ListFilter";
import type { RangeField } from "../bindings/RangeField";
import type { RangeFilter } from "../bindings/RangeFilter";
import { storesOf } from "./copies";

/** `ListFilter.values` entry for games without a value (the core's `NO_VALUE`). */
export const NO_VALUE = ":none";

export type FieldSection = "about" | "scores" | "you";

export const SECTION_LABELS: Record<FieldSection, string> = {
  about: "About the game",
  scores: "Scores & length",
  you: "You",
};

/** How a range field's numbers are shown and typed. */
export type RangeUnit = "number" | "year" | "hours" | "gigabytes" | "date" | "score";

export interface RangePreset {
  label: string;
  min: number | null;
  max: number | null;
}

export type FieldInfo =
  | {
      kind: "list";
      field: ListField;
      label: string;
      /** For "+ Add tag" and "No tags". */
      noun: string;
      plural: string;
      section: FieldSection;
    }
  | {
      kind: "range";
      field: RangeField;
      label: string;
      section: FieldSection;
      unit: RangeUnit;
      presets: RangePreset[];
    };

export type ListInfo = Extract<FieldInfo, { kind: "list" }>;
export type RangeInfo = Extract<FieldInfo, { kind: "range" }>;

const HOUR = 3600;
const GB = 1024 ** 3;
const DAY = 86400;

const list = (
  field: ListField,
  label: string,
  noun: string,
  plural: string,
  section: FieldSection,
): FieldInfo => ({ kind: "list", field, label, noun, plural, section });

const range = (
  field: RangeField,
  label: string,
  section: FieldSection,
  unit: RangeUnit,
  presets: RangePreset[] = [],
): FieldInfo => ({ kind: "range", field, label, section, unit, presets });

/** Unix seconds `days` ago, at the time the preset is chosen. */
export const daysAgo = (days: number, now = Date.now()) => Math.floor(now / 1000) - days * DAY;

/** Every field the filter panel can add, in picker order. */
export const FIELDS: FieldInfo[] = [
  list("genre", "Genre", "genre", "genres", "about"),
  list("tag", "Tags", "tag", "tags", "about"),
  list("feature", "Features", "feature", "features", "about"),
  list("series", "Series", "series", "series", "about"),
  list("developer", "Developer", "developer", "developers", "about"),
  list("publisher", "Publisher", "publisher", "publishers", "about"),
  list("platform", "Platform", "platform", "platforms", "about"),
  list("ageRating", "Age rating", "age rating", "age ratings", "about"),
  list("gameMode", "Game mode", "game mode", "game modes", "about"),
  list("theme", "Theme", "theme", "themes", "about"),
  list("perspective", "Perspective", "perspective", "perspectives", "about"),
  range("releaseYear", "Release year", "about", "year"),
  range("criticScore", "Critic score", "scores", "score", [
    { label: "75 +", min: 75, max: null },
    { label: "90 +", min: 90, max: null },
  ]),
  range("communityScore", "Community score", "scores", "score", [
    { label: "75 +", min: 75, max: null },
    { label: "90 +", min: 90, max: null },
  ]),
  range("timeToBeat", "Time to beat", "scores", "hours", [
    { label: "< 5 h", min: null, max: 5 * HOUR },
    { label: "5–20 h", min: 5 * HOUR, max: 20 * HOUR },
    { label: "50 h +", min: 50 * HOUR, max: null },
  ]),
  list("source", "Store", "store", "stores", "you"),
  list("category", "Category", "category", "categories", "you"),
  list("completionStatus", "Completion status", "status", "statuses", "you"),
  range("userScore", "Your score", "you", "score"),
  range("playtime", "Playtime", "you", "hours", [
    { label: "Never", min: 0, max: 0 },
    { label: "< 1 h", min: null, max: HOUR },
    { label: "10 h +", min: 10 * HOUR, max: null },
  ]),
  range("playCount", "Play count", "you", "number", [
    { label: "Never", min: 0, max: 0 },
    { label: "10 +", min: 10, max: null },
  ]),
  range("lastPlayed", "Last played", "you", "date"),
  range("dateAdded", "Date added", "you", "date"),
  range("installSize", "Install size", "you", "gigabytes", [
    { label: "< 1 GB", min: null, max: GB },
    { label: "50 GB +", min: 50 * GB, max: null },
  ]),
];

/** The date presets are relative to now, so they are made when shown. */
export function presetsOf(info: RangeInfo, now = Date.now()): RangePreset[] {
  if (info.unit !== "date") return info.presets;
  return [
    { label: "Last 7 days", min: daysAgo(7, now), max: null },
    { label: "Last 30 days", min: daysAgo(30, now), max: null },
    { label: "Over a year ago", min: null, max: daysAgo(365, now) },
  ];
}

export function listInfo(field: ListField): ListInfo {
  return FIELDS.find((f): f is ListInfo => f.kind === "list" && f.field === field) as ListInfo;
}

export function rangeInfo(field: RangeField): RangeInfo {
  return FIELDS.find((f): f is RangeInfo => f.kind === "range" && f.field === field) as RangeInfo;
}

export function infoOf(node: Exclude<FilterNode, { kind: "group" }>): FieldInfo {
  return node.kind === "list" ? listInfo(node.field) : rangeInfo(node.field);
}

/** A new, empty filter for a field. */
export function newFilter(info: FieldInfo): FilterNode {
  return info.kind === "list"
    ? { kind: "list", field: info.field, mode: "any", values: [] }
    : { kind: "range", field: info.field, min: null, max: null };
}

export const emptyGroup = (): FilterGroup => ({ mode: "all", items: [] });

// ---- editing the tree by path (indices from the root group) ----

export type Path = readonly number[];

export function nodeAt(group: FilterGroup, path: Path): FilterNode | FilterGroup | undefined {
  let current: FilterNode | FilterGroup = group;
  for (const index of path) {
    if (!("items" in current)) return undefined;
    const next: FilterNode | undefined = current.items[index];
    if (!next) return undefined;
    current = next;
  }
  return current;
}

/** Replaces the items of the group at `path`. */
function withItems(
  group: FilterGroup,
  path: Path,
  change: (items: FilterNode[]) => FilterNode[],
): FilterGroup {
  if (path.length === 0) return { ...group, items: change([...group.items]) };
  const [head, ...rest] = path;
  return {
    ...group,
    items: group.items.map((item, i) =>
      i === head && item.kind === "group" ? { ...item, ...withItems(item, rest, change) } : item,
    ),
  };
}

/** Replaces the node at `path` (not the root). */
export function updateAt(
  group: FilterGroup,
  path: Path,
  change: (node: FilterNode) => FilterNode,
): FilterGroup {
  const parent = path.slice(0, -1);
  const index = path[path.length - 1] ?? -1;
  return withItems(group, parent, (items) => items.map((n, i) => (i === index ? change(n) : n)));
}

export function removeAt(group: FilterGroup, path: Path): FilterGroup {
  const index = path[path.length - 1] ?? -1;
  return withItems(group, path.slice(0, -1), (items) => items.filter((_, i) => i !== index));
}

/** Appends a node to the group at `groupPath`. */
export function appendTo(group: FilterGroup, groupPath: Path, node: FilterNode): FilterGroup {
  return withItems(group, groupPath, (items) => [...items, node]);
}

/** Sets the all/any mode of the group at `groupPath` (the root too). */
export function setMode(
  group: FilterGroup,
  groupPath: Path,
  mode: FilterGroup["mode"],
): FilterGroup {
  if (groupPath.length === 0) return { ...group, mode };
  return updateAt(group, groupPath, (node) => ({ ...node, mode }) as FilterNode);
}

/** Replaces a group by its items, in its place. */
export function ungroupAt(group: FilterGroup, path: Path): FilterGroup {
  const inner = nodeAt(group, path);
  if (!inner || !("items" in inner)) return group;
  const index = path[path.length - 1] ?? -1;
  return withItems(group, path.slice(0, -1), (items) => [
    ...items.slice(0, index),
    ...inner.items,
    ...items.slice(index + 1),
  ]);
}

/** Puts a copy of the node right after it. */
export function duplicateAt(group: FilterGroup, path: Path): FilterGroup {
  const node = nodeAt(group, path);
  if (!node || !("kind" in node)) return group;
  const index = path[path.length - 1] ?? -1;
  const copy = structuredClone(node) as FilterNode;
  return withItems(group, path.slice(0, -1), (items) => [
    ...items.slice(0, index + 1),
    copy,
    ...items.slice(index + 1),
  ]);
}

// ---- reading the tree ----

/** Whether a node narrows anything (a list with values, a range with a bound, a group of those). */
export function isActive(node: FilterNode | FilterGroup): boolean {
  if (!("kind" in node) || node.kind === "group") return node.items.some(isActive);
  if (node.kind === "list") return node.values.length > 0;
  return node.min !== null || node.max !== null;
}

/** Active filters in the tree: each list or range counts once, groups count their members. */
export function activeTreeCount(group: FilterGroup): number {
  return group.items.reduce(
    (sum, node) => sum + (node.kind === "group" ? activeTreeCount(node) : isActive(node) ? 1 : 0),
    0,
  );
}

/** Drops empty filters and groups, and groups of one (their member takes their place). */
export function pruneTree(group: FilterGroup): FilterGroup {
  const items: FilterNode[] = [];
  for (const node of group.items) {
    if (node.kind !== "group") {
      if (isActive(node)) items.push(node);
      continue;
    }
    const inner = pruneTree(node);
    const [only] = inner.items;
    if (inner.items.length === 1 && only) items.push(only);
    else if (inner.items.length > 1) items.push({ kind: "group", ...inner });
  }
  return { mode: group.mode, items };
}

function sameNode(a: FilterNode, b: FilterNode): boolean {
  if (a.kind === "group" || b.kind === "group") {
    return a.kind === b.kind && sameTree(a as FilterGroup, b as FilterGroup);
  }
  if (a.kind === "range" || b.kind === "range") {
    if (a.kind !== "range" || b.kind !== "range") return false;
    return a.field === b.field && a.min === b.min && a.max === b.max;
  }
  return (
    a.field === b.field &&
    a.mode === b.mode &&
    a.values.length === b.values.length &&
    a.values.every((v, i) => v === b.values[i])
  );
}

/** Whether two trees are the same, rows not yet filled in included (they are edits too). */
export function sameTree(a: FilterGroup, b: FilterGroup): boolean {
  return (
    a.mode === b.mode &&
    a.items.length === b.items.length &&
    a.items.every((node, i) => {
      const other = b.items[i];
      return other !== undefined && sameNode(node, other);
    })
  );
}

// ---- matching a game (mirrors the core's SQL, for picking the view of a game) ----

/** The core's value key: letters, digits and `+`, lowercased. */
export function valueKey(value: string): string {
  return [...value]
    .filter((c) => /[\p{L}\p{N}+]/u.test(c))
    .join("")
    .toLowerCase();
}

function listValues(game: Game, field: ListField): string[] {
  switch (field) {
    case "source":
      return storesOf(game);
    case "category":
      return game.categories;
    case "completionStatus":
      return game.completionStatus === null ? [] : [game.completionStatus];
    default: {
      const names: Record<string, string[]> = {
        genre: game.genres,
        tag: game.tags,
        feature: game.features,
        series: game.series,
        platform: game.platforms,
        ageRating: game.ageRatings,
        developer: game.developers,
        publisher: game.publishers,
        gameMode: game.gameModes,
        theme: game.themes,
        perspective: game.perspectives,
      };
      return (names[field] ?? []).map(valueKey);
    }
  }
}

const unixSeconds = (iso: string | null) => {
  if (iso === null) return null;
  const ms = Date.parse(iso);
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000);
};

/** A game's number for a range field, `null` without a value. */
export function rangeValue(game: Game, field: RangeField): number | null {
  switch (field) {
    case "releaseYear": {
      const year = Number.parseInt(game.releaseDate?.slice(0, 4) ?? "", 10);
      return Number.isNaN(year) ? null : year;
    }
    case "playtime":
      return game.playtimeSeconds;
    case "playCount":
      return game.playCount;
    case "lastPlayed":
      return unixSeconds(game.lastPlayed);
    case "dateAdded":
      return unixSeconds(game.dateAdded);
    case "criticScore":
      return game.criticScore?.value ?? null;
    case "communityScore":
      return game.communityScore?.value ?? null;
    case "userScore":
      return game.userScore;
    case "timeToBeat":
      return game.timeToBeat?.main ?? null;
    case "installSize":
      return game.installSize;
  }
}

function matchesList(filter: ListFilter, game: Game): boolean {
  const have = listValues(game, filter.field);
  const has = (value: string) =>
    value === NO_VALUE ? filter.field !== "source" && have.length === 0 : have.includes(value);
  if (filter.mode === "all") return filter.values.every(has);
  const any = filter.values.some(has);
  return filter.mode === "any" ? any : !any;
}

function matchesRange(filter: RangeFilter, game: Game): boolean {
  const value = rangeValue(game, filter.field);
  if (value === null) return false;
  if (filter.min !== null && value < filter.min) return false;
  if (filter.max !== null && value > filter.max) return false;
  return true;
}

/** Whether a game passes the filter tree; empty rows and groups are left out like in the core. */
export function matchesTree(group: FilterGroup, game: Game): boolean {
  const results = group.items
    .filter(isActive)
    .map((node) =>
      node.kind === "group"
        ? matchesTree(node, game)
        : node.kind === "list"
          ? matchesList(node, game)
          : matchesRange(node, game),
    );
  if (results.length === 0) return true;
  return group.mode === "all" ? results.every(Boolean) : results.some(Boolean);
}

// ---- showing ranges ----

/** A stored range bound as the number typed in the field (hours, GB, ...). */
export function toInput(unit: RangeUnit, value: number | null): string {
  if (value === null) return "";
  switch (unit) {
    case "hours":
      return String(Math.round((value / HOUR) * 10) / 10);
    case "gigabytes":
      return String(Math.round((value / GB) * 10) / 10);
    case "date":
      return new Date(value * 1000).toISOString().slice(0, 10);
    default:
      return String(value);
  }
}

/** A typed bound in stored units; `null` for an empty or unreadable field. */
export function fromInput(unit: RangeUnit, text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  if (unit === "date") return unixSeconds(trimmed);
  const number = Number(trimmed.replace(",", "."));
  if (!Number.isFinite(number) || number < 0) return null;
  switch (unit) {
    case "hours":
      return Math.round(number * HOUR);
    case "gigabytes":
      return Math.round(number * GB);
    default:
      return Math.round(number);
  }
}

const UNIT_SUFFIX: Partial<Record<RangeUnit, string>> = { hours: " h", gigabytes: " GB" };

/** "2 – 50 h", "10 h +", "up to 1 GB", "since 2024-01-01". */
export function describeRange(info: RangeInfo, min: number | null, max: number | null): string {
  const suffix = UNIT_SUFFIX[info.unit] ?? "";
  const show = (value: number) => toInput(info.unit, value);
  if (info.unit === "date") {
    if (min !== null && max !== null) return `${show(min)} – ${show(max)}`;
    if (min !== null) return `since ${show(min)}`;
    if (max !== null) return `before ${show(max)}`;
    return "";
  }
  if (min !== null && max !== null) {
    return min === max ? `${show(min)}${suffix}` : `${show(min)} – ${show(max)}${suffix}`;
  }
  if (min !== null) return `${show(min)}${suffix} +`;
  if (max !== null) return `up to ${show(max)}${suffix}`;
  return "";
}

/** One pill under the toolbar: a field and what it is set to. */
export interface PillPart {
  label: string;
  value: string;
}

/** `index` is the node's place in the root group (for removing it). */
export type Pill = { index: number; parts: PillPart[]; joiner: "and" | "or"; grouped: boolean };

/** A filter's pill text; `name` turns a list value key into its name. */
export function describeNode(
  node: Exclude<FilterNode, { kind: "group" }>,
  name: (field: ListField, key: string) => string,
): PillPart {
  const info = infoOf(node);
  if (node.kind === "range") {
    return { label: info.label, value: describeRange(info as RangeInfo, node.min, node.max) };
  }
  const listed = info as ListInfo;
  const names = node.values.map((key) =>
    key === NO_VALUE ? `No ${listed.plural}` : name(node.field, key),
  );
  const prefix = node.mode === "none" ? "not " : "";
  const joiner = node.mode === "all" ? " and " : ", ";
  return { label: listed.label, value: prefix + names.join(joiner) };
}

/**
 * The pills of a tree: each active filter of the root is one pill, each active group one pill
 * listing its members. Nested groups inside a group are summed up as "(…)".
 */
export function treePills(
  group: FilterGroup,
  name: (field: ListField, key: string) => string,
): Pill[] {
  const pills: Pill[] = [];
  for (const [index, node] of group.items.entries()) {
    if (!isActive(node)) continue;
    if (node.kind !== "group") {
      pills.push({ index, parts: [describeNode(node, name)], joiner: "and", grouped: false });
      continue;
    }
    const parts = node.items
      .filter(isActive)
      .map((inner) =>
        inner.kind === "group"
          ? { label: "", value: `(${activeTreeCount(inner)} filters)` }
          : describeNode(inner, name),
      );
    pills.push({ index, parts, joiner: node.mode === "all" ? "and" : "or", grouped: true });
  }
  return pills;
}
