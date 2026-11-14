import type { FilterGroup } from "../bindings/FilterGroup";
import type { FilterNode } from "../bindings/FilterNode";
import type { Game } from "../bindings/Game";
import type { ListField } from "../bindings/ListField";
import type { ListFilter } from "../bindings/ListFilter";
import type { RangeField } from "../bindings/RangeField";
import type { RangeFilter } from "../bindings/RangeFilter";
import type { TextField } from "../bindings/TextField";
import type { TextFilter } from "../bindings/TextFilter";
import type { TextOp } from "../bindings/TextOp";
import { translator } from "../i18n";
import { libraryFilters } from "../i18n/libraryFilters";
import { storesOf } from "./copies";

const tr = translator(libraryFilters);

/** `ListFilter.values` entry for games without a value (the core's `NO_VALUE`). */
export const NO_VALUE = ":none";

export type FieldSection = "text" | "about" | "scores" | "you";

/** Section headings of the field list; read at use time, so they follow the language. */
export const SECTION_LABELS: Record<FieldSection, string> = {
  get text() {
    return tr("sectionText");
  },
  get about() {
    return tr("sectionAbout");
  },
  get scores() {
    return tr("sectionScores");
  },
  get you() {
    return tr("sectionYou");
  },
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
    }
  | {
      kind: "text";
      field: TextField;
      label: string;
      section: FieldSection;
    };

export type ListInfo = Extract<FieldInfo, { kind: "list" }>;
export type RangeInfo = Extract<FieldInfo, { kind: "range" }>;
export type TextInfo = Extract<FieldInfo, { kind: "text" }>;

const HOUR = 3600;
const GB = 1024 ** 3;
const DAY = 86400;

type ListKey =
  | "genre"
  | "tag"
  | "feature"
  | "series"
  | "developer"
  | "publisher"
  | "platform"
  | "ageRating"
  | "gameMode"
  | "theme"
  | "perspective"
  | "source"
  | "category"
  | "completionStatus";

type RangeKey =
  | "releaseYear"
  | "criticScore"
  | "communityScore"
  | "timeToBeat"
  | "userScore"
  | "playtime"
  | "playCount"
  | "lastPlayed"
  | "dateAdded"
  | "installSize";

// The texts are getters: they are looked up when read, in the language of that moment.
const list = (field: ListField, key: ListKey, section: FieldSection): FieldInfo => ({
  kind: "list",
  field,
  section,
  get label() {
    return tr(key);
  },
  get noun() {
    return tr(`${key}Noun`);
  },
  get plural() {
    return tr(`${key}Plural`);
  },
});

const range = (
  field: RangeField,
  key: RangeKey,
  section: FieldSection,
  unit: RangeUnit,
  presets: RangePreset[] = [],
): FieldInfo => ({
  kind: "range",
  field,
  section,
  unit,
  presets,
  get label() {
    return tr(key);
  },
});

type TextKey =
  | "textAny"
  | "textName"
  | "textSortingName"
  | "textDescription"
  | "textNotes"
  | "textInstallDir"
  | "textSourceGameId"
  | "textLaunchTarget"
  | "textLaunchArgs";

const textField = (field: TextField, key: TextKey): FieldInfo => ({
  kind: "text",
  field,
  section: "text",
  get label() {
    return tr(key);
  },
});

/** The names of a list field's values, matched as text ("Developer (text)"). */
const listText = (field: TextField & ListField, key: ListKey): FieldInfo => ({
  kind: "text",
  field,
  section: "text",
  get label() {
    return tr("listText", { label: tr(key) });
  },
});

/** The comparisons of a text filter, in menu order. */
export const TEXT_OPS: readonly TextOp[] = [
  "contains",
  "startsWith",
  "endsWith",
  "equals",
  "matches",
  "isEmpty",
  "hasValue",
];

const OP_KEYS = {
  contains: "opContains",
  startsWith: "opStartsWith",
  endsWith: "opEndsWith",
  equals: "opEquals",
  matches: "opMatches",
  isEmpty: "opIsEmpty",
  hasValue: "opHasValue",
} as const satisfies Record<TextOp, string>;

/** "contains", "starts with", ... in the current language. */
export const opLabel = (op: TextOp) => tr(OP_KEYS[op]);

/** Whether an operation compares with typed text (the empty checks don't). */
export const needsValue = (op: TextOp) => op !== "isEmpty" && op !== "hasValue";

/** A preset with a fixed label (numbers and units need no translation). */
const fixed = (label: string, min: number | null, max: number | null): RangePreset => ({
  label,
  min,
  max,
});

const neverPreset = (): RangePreset => ({
  get label() {
    return tr("never");
  },
  min: 0,
  max: 0,
});

/** Unix seconds `days` ago, at the time the preset is chosen. */
export const daysAgo = (days: number, now = Date.now()) => Math.floor(now / 1000) - days * DAY;

/** Every field the filter panel can add, in picker order. */
export const FIELDS: FieldInfo[] = [
  textField("anyText", "textAny"),
  textField("name", "textName"),
  textField("sortingName", "textSortingName"),
  textField("description", "textDescription"),
  textField("notes", "textNotes"),
  listText("developer", "developer"),
  listText("publisher", "publisher"),
  listText("series", "series"),
  listText("genre", "genre"),
  listText("tag", "tag"),
  listText("feature", "feature"),
  listText("platform", "platform"),
  listText("ageRating", "ageRating"),
  listText("gameMode", "gameMode"),
  listText("theme", "theme"),
  listText("perspective", "perspective"),
  textField("installDir", "textInstallDir"),
  textField("sourceGameId", "textSourceGameId"),
  textField("launchTarget", "textLaunchTarget"),
  textField("launchArgs", "textLaunchArgs"),
  list("genre", "genre", "about"),
  list("tag", "tag", "about"),
  list("feature", "feature", "about"),
  list("series", "series", "about"),
  list("developer", "developer", "about"),
  list("publisher", "publisher", "about"),
  list("platform", "platform", "about"),
  list("ageRating", "ageRating", "about"),
  list("gameMode", "gameMode", "about"),
  list("theme", "theme", "about"),
  list("perspective", "perspective", "about"),
  range("releaseYear", "releaseYear", "about", "year"),
  range("criticScore", "criticScore", "scores", "score", [
    fixed("75 +", 75, null),
    fixed("90 +", 90, null),
  ]),
  range("communityScore", "communityScore", "scores", "score", [
    fixed("75 +", 75, null),
    fixed("90 +", 90, null),
  ]),
  range("timeToBeat", "timeToBeat", "scores", "hours", [
    fixed("< 5 h", null, 5 * HOUR),
    fixed("5–20 h", 5 * HOUR, 20 * HOUR),
    fixed("50 h +", 50 * HOUR, null),
  ]),
  list("source", "source", "you"),
  list("category", "category", "you"),
  list("completionStatus", "completionStatus", "you"),
  range("userScore", "userScore", "you", "score"),
  range("playtime", "playtime", "you", "hours", [
    neverPreset(),
    fixed("< 1 h", null, HOUR),
    fixed("10 h +", 10 * HOUR, null),
  ]),
  range("playCount", "playCount", "you", "number", [neverPreset(), fixed("10 +", 10, null)]),
  range("lastPlayed", "lastPlayed", "you", "date"),
  range("dateAdded", "dateAdded", "you", "date"),
  range("installSize", "installSize", "you", "gigabytes", [
    fixed("< 1 GB", null, GB),
    fixed("50 GB +", 50 * GB, null),
  ]),
];

/** The date presets are relative to now, so they are made when shown. */
export function presetsOf(info: RangeInfo, now = Date.now()): RangePreset[] {
  if (info.unit !== "date") return info.presets;
  return [
    fixed(tr("last7Days"), daysAgo(7, now), null),
    fixed(tr("last30Days"), daysAgo(30, now), null),
    fixed(tr("overYearAgo"), null, daysAgo(365, now)),
  ];
}

export function listInfo(field: ListField): ListInfo {
  return FIELDS.find((f): f is ListInfo => f.kind === "list" && f.field === field) as ListInfo;
}

export function rangeInfo(field: RangeField): RangeInfo {
  return FIELDS.find((f): f is RangeInfo => f.kind === "range" && f.field === field) as RangeInfo;
}

export function textInfo(field: TextField): TextInfo {
  return FIELDS.find((f): f is TextInfo => f.kind === "text" && f.field === field) as TextInfo;
}

export function infoOf(node: Exclude<FilterNode, { kind: "group" }>): FieldInfo {
  switch (node.kind) {
    case "list":
      return listInfo(node.field);
    case "range":
      return rangeInfo(node.field);
    case "text":
      return textInfo(node.field);
  }
}

/** A new, empty filter for a field. */
export function newFilter(info: FieldInfo): FilterNode {
  switch (info.kind) {
    case "list":
      return { kind: "list", field: info.field, mode: "any", values: [] };
    case "range":
      return { kind: "range", field: info.field, min: null, max: null };
    case "text":
      return { kind: "text", field: info.field, op: "contains", value: "" };
  }
}

/** A text filter's pattern as a JavaScript regular expression; `null` when it does not compile. */
export function textPattern(filter: TextFilter): RegExp | null {
  try {
    return new RegExp(filter.value, filter.caseSensitive === true ? "u" : "iu");
  } catch {
    return null;
  }
}

/** Flips a filter or group: it then has the games it left out. */
export function toggleNegate(group: FilterGroup, path: Path): FilterGroup {
  if (path.length === 0) return { ...group, negate: group.negate !== true };
  return updateAt(group, path, (node) => ({ ...node, negate: node.negate !== true }));
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

// ---- moving (drag and drop, and the move menu) ----

/** Where a dragged node goes relative to the one under the pointer. */
export type DropPlace = "before" | "after" | "into";

export const samePath = (a: Path, b: Path) =>
  a.length === b.length && a.every((step, i) => step === b[i]);

const startsWithPath = (path: Path, prefix: Path) =>
  path.length >= prefix.length && prefix.every((step, i) => step === path[i]);

/** Whether `from` may go to `place` of `target`: never into itself or one of its own groups. */
export function canMove(tree: FilterGroup, from: Path, target: Path, place: DropPlace): boolean {
  if (from.length === 0 || startsWithPath(target, from)) return false;
  if (place !== "into") return target.length > 0;
  const group = nodeAt(tree, target);
  return group !== undefined && "items" in group;
}

/** Moves the node at `from` before, after or to the end of (`into`) the node at `target`. */
export function moveNode(
  tree: FilterGroup,
  from: Path,
  target: Path,
  place: DropPlace,
): FilterGroup {
  const node = nodeAt(tree, from);
  if (!node || !("kind" in node) || !canMove(tree, from, target, place)) return tree;
  // The destination group and index, before the node is taken out.
  let parent: number[];
  let index: number;
  if (place === "into") {
    const group = nodeAt(tree, target) as FilterGroup;
    parent = [...target];
    index = group.items.length;
  } else {
    parent = target.slice(0, -1);
    index = (target.at(-1) ?? 0) + (place === "after" ? 1 : 0);
  }
  const fromParent = from.slice(0, -1);
  const fromIndex = from.at(-1) ?? 0;
  // Taking the node out shifts what came after it in its group.
  const depth = fromParent.length;
  if (parent.length > depth && startsWithPath(parent, fromParent)) {
    const step = parent[depth] ?? 0;
    if (step > fromIndex) parent[depth] = step - 1;
  }
  if (samePath(parent, fromParent) && index > fromIndex) index -= 1;
  const removed = removeAt(tree, from);
  return withItems(removed, parent, (items) => [
    ...items.slice(0, index),
    node,
    ...items.slice(index),
  ]);
}

/** The keyboard moves of a node: up, down, out of its group, into the group above it. */
export function moveChoices(
  tree: FilterGroup,
  path: Path,
): { up: boolean; down: boolean; out: boolean; intoAbove: boolean } {
  const index = path.at(-1) ?? 0;
  const parent = nodeAt(tree, path.slice(0, -1));
  const siblings = parent && "items" in parent ? parent.items : [];
  return {
    up: index > 0,
    down: index < siblings.length - 1,
    out: path.length > 1,
    intoAbove: index > 0 && siblings[index - 1]?.kind === "group",
  };
}

/** Applies a keyboard move (see [`moveChoices`]). */
export function moveBy(
  tree: FilterGroup,
  path: Path,
  how: "up" | "down" | "out" | "intoAbove",
): FilterGroup {
  const index = path.at(-1) ?? 0;
  const sibling = (offset: number) => [...path.slice(0, -1), index + offset];
  switch (how) {
    case "up":
      return moveNode(tree, path, sibling(-1), "before");
    case "down":
      return moveNode(tree, path, sibling(1), "after");
    case "out":
      return moveNode(tree, path, path.slice(0, -1), "after");
    case "intoAbove":
      return moveNode(tree, path, sibling(-1), "into");
  }
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
  if (node.kind === "text") return !needsValue(node.op) || node.value !== "";
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
    // A negated group of one is its member, negated once more.
    if (inner.items.length === 1 && only) {
      items.push(inner.negate === true ? { ...only, negate: only.negate !== true } : only);
    } else if (inner.items.length > 1) items.push({ kind: "group", ...inner });
  }
  return group.negate === true
    ? { mode: group.mode, items, negate: true }
    : { mode: group.mode, items };
}

const negated = (node: { negate?: boolean }) => node.negate === true;

function sameNode(a: FilterNode, b: FilterNode): boolean {
  if (a.kind === "group" || b.kind === "group") {
    return a.kind === b.kind && sameTree(a as FilterGroup, b as FilterGroup);
  }
  if (a.kind !== b.kind || a.field !== b.field || negated(a) !== negated(b)) return false;
  if (a.kind === "range" && b.kind === "range") return a.min === b.min && a.max === b.max;
  if (a.kind === "text" && b.kind === "text") {
    return (
      a.op === b.op &&
      a.value === b.value &&
      (a.caseSensitive === true) === (b.caseSensitive === true)
    );
  }
  if (a.kind === "list" && b.kind === "list") {
    return (
      a.mode === b.mode &&
      a.values.length === b.values.length &&
      a.values.every((v, i) => v === b.values[i])
    );
  }
  return false;
}

/** Whether two trees are the same, rows not yet filled in included (they are edits too). */
export function sameTree(a: FilterGroup, b: FilterGroup): boolean {
  return (
    a.mode === b.mode &&
    negated(a) === negated(b) &&
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
    default:
      return (listNames(game, field) ?? []).map(valueKey);
  }
}

/** The value names of a metadata list field; `undefined` for other fields. */
function listNames(game: Game, field: string): string[] | undefined {
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
  return names[field];
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

/** The texts behind a text field: one for most, several for "any text", stores and lists. */
function textValues(game: Game, field: TextField): (string | null)[] {
  switch (field) {
    case "name":
      return [game.name];
    case "sortingName":
      return [game.sortingName];
    case "description":
      return [game.description];
    case "notes":
      return [game.notes];
    case "installDir":
      return [game.installDir];
    case "anyText":
      return [game.name, game.description, game.notes];
    case "sourceGameId":
      return game.copies.map((copy) => copy.sourceGameId);
    case "launchTarget":
      return [game.playAction?.target ?? null];
    case "launchArgs":
      return [game.playAction?.args ?? null];
    default:
      return listNames(game, field) ?? [];
  }
}

/** One text against a filter that compares (not an empty check). */
function textHit(filter: TextFilter, value: string): boolean {
  if (filter.op === "matches") return textPattern(filter)?.test(value) ?? false;
  const fold = (s: string) => (filter.caseSensitive === true ? s : s.toLowerCase());
  const [have, want] = [fold(value), fold(filter.value)];
  switch (filter.op) {
    case "contains":
      return have.includes(want);
    case "startsWith":
      return have.startsWith(want);
    case "endsWith":
      return have.endsWith(want);
    default:
      return have === want;
  }
}

/**
 * A text filter (mirrors the core's `qk_text`; JavaScript and Rust regexes differ a little): any
 * of the field's texts matching is enough; "is empty" needs all of them empty.
 */
export function matchesText(filter: TextFilter, game: Game): boolean {
  const values = textValues(game, filter.field);
  if (!needsValue(filter.op)) {
    const empty = values.every((v) => v === null || v.trim() === "");
    return empty === (filter.op === "isEmpty");
  }
  return values.some((value) => value !== null && textHit(filter, value));
}

function matchesNode(node: FilterNode, game: Game): boolean {
  switch (node.kind) {
    case "group":
      return matchesTree(node, game);
    case "list":
      return matchesList(node, game) !== negated(node);
    case "range":
      return matchesRange(node, game) !== negated(node);
    case "text":
      return matchesText(node, game) !== negated(node);
  }
}

/** Whether a game passes the filter tree; empty rows and groups are left out like in the core. */
export function matchesTree(group: FilterGroup, game: Game): boolean {
  const results = group.items.filter(isActive).map((node) => matchesNode(node, game));
  if (results.length === 0) return true;
  const hit = group.mode === "all" ? results.every(Boolean) : results.some(Boolean);
  return hit !== negated(group);
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
    if (min !== null) return tr("since", { date: show(min) });
    if (max !== null) return tr("before", { date: show(max) });
    return "";
  }
  if (min !== null && max !== null) {
    return min === max ? `${show(min)}${suffix}` : `${show(min)} – ${show(max)}${suffix}`;
  }
  if (min !== null) return `${show(min)}${suffix} +`;
  if (max !== null) return tr("upTo", { value: `${show(max)}${suffix}` });
  return "";
}

/** One pill under the toolbar: a field and what it is set to. */
export interface PillPart {
  label: string;
  value: string;
}

/**
 * `index` is the node's place in the root group (for removing it); `negated` marks a group that
 * has the games its members leave out.
 */
export type Pill = {
  index: number;
  parts: PillPart[];
  joiner: "and" | "or";
  grouped: boolean;
  negated: boolean;
};

/** "contains “trine”", with "(match case)" when it compares case. */
export function describeText(filter: TextFilter): string {
  if (!needsValue(filter.op)) return opLabel(filter.op);
  const text = tr("textValue", { op: opLabel(filter.op), value: filter.value });
  return filter.caseSensitive === true ? `${text} (${tr("matchCaseShort")})` : text;
}

/** A filter's pill text; `name` turns a list value key into its name. */
export function describeNode(
  node: Exclude<FilterNode, { kind: "group" }>,
  name: (field: ListField, key: string) => string,
): PillPart {
  const info = infoOf(node);
  const not = (value: string) => `${tr("notWord")} ${value}`;
  if (node.kind === "range") {
    const value = describeRange(info as RangeInfo, node.min, node.max);
    return { label: info.label, value: negated(node) ? not(value) : value };
  }
  if (node.kind === "text") {
    const value = describeText(node);
    return { label: info.label, value: negated(node) ? not(value) : value };
  }
  const listed = info as ListInfo;
  const names = node.values.map((key) =>
    key === NO_VALUE ? tr("noValue", { plural: listed.plural }) : name(node.field, key),
  );
  const joiner = node.mode === "all" ? tr("and") : ", ";
  const joined = names.join(joiner);
  // "None of" and a negation cancel out.
  const none = node.mode === "none";
  const flipped = none !== negated(node);
  return {
    label: listed.label,
    value: flipped
      ? none
        ? tr("not", { names: joined })
        : not(node.mode === "all" ? joined : `(${joined})`)
      : joined,
  };
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
      pills.push({
        index,
        parts: [describeNode(node, name)],
        joiner: "and",
        grouped: false,
        negated: false,
      });
      continue;
    }
    const parts = node.items
      .filter(isActive)
      .map((inner) =>
        inner.kind === "group"
          ? { label: "", value: tr("nestedFilters", { count: activeTreeCount(inner) }) }
          : describeNode(inner, name),
      );
    pills.push({
      index,
      parts,
      joiner: node.mode === "all" ? "and" : "or",
      grouped: true,
      negated: negated(node),
    });
  }
  return pills;
}

/** "and" / "or" between the members of a group, as a word of the current language. */
export function joinerWord(joiner: "and" | "or"): string {
  return tr(joiner).trim();
}

/** What a picker button shows: the first `max` picked names, and how many more are picked. */
export function pickedNames(names: readonly string[], max = 2): { shown: string; more: number } {
  return { shown: names.slice(0, max).join(", "), more: Math.max(0, names.length - max) };
}
