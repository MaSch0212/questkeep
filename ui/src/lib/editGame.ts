import { translator } from "../i18n";
import { gameEdit } from "../i18n/gameEdit";
import type { GameEditInfo } from "../bindings/GameEditInfo";
import type { GameLink } from "../bindings/GameLink";
import type { GameMetadata } from "../bindings/GameMetadata";
import type { ListField } from "../bindings/ListField";
import type { MetadataField } from "../bindings/MetadataField";
import type { UserEdits } from "../bindings/UserEdits";

const tr = translator(gameEdit);

/** The list fields of the dialog with their labels and the filter field for suggestions. */
export type ListMetaField =
  | "developers"
  | "publishers"
  | "genres"
  | "series"
  | "tags"
  | "features"
  | "platforms"
  | "ageRatings"
  | "gameModes"
  | "perspectives"
  | "themes";

/** The dictionary key of a list field's label. */
export type ListLabel = Extract<keyof typeof gameEdit.en, `field${string}`>;

export const LIST_FIELDS: {
  field: ListMetaField;
  label: ListLabel;
  values: ListField;
  wide?: boolean;
}[] = [
  { field: "developers", label: "fieldDevelopers", values: "developer" },
  { field: "publishers", label: "fieldPublishers", values: "publisher" },
  { field: "genres", label: "fieldGenres", values: "genre" },
  { field: "series", label: "fieldSeries", values: "series" },
  { field: "tags", label: "fieldTags", values: "tag", wide: true },
  { field: "features", label: "fieldFeatures", values: "feature", wide: true },
  { field: "platforms", label: "fieldPlatforms", values: "platform" },
  { field: "ageRatings", label: "fieldAgeRating", values: "ageRating" },
  { field: "gameModes", label: "fieldGameModes", values: "gameMode" },
  { field: "perspectives", label: "fieldPerspective", values: "perspective" },
  { field: "themes", label: "fieldThemes", values: "theme", wide: true },
];

/** Which dialog section shows which metadata field (for the "N edited" counts). */
export const SECTION_FIELDS: Record<"general" | "details", MetadataField[]> = {
  general: ["releaseDate", "description"],
  details: [
    ...LIST_FIELDS.map((f) => f.field),
    "criticScore",
    "communityScore",
    "timeToBeat",
    "installSize",
  ],
};

const SOURCE_NAMES: Record<string, string> = {
  igdb: "IGDB",
  hltb: "HowLongToBeat",
  steamgriddb: "SteamGridDB",
};

/** Where a field's automatic value comes from, as shown next to it. */
export function sourceLabel(source: string | undefined, nameOf: (id: string) => string): string {
  if (source === undefined) return tr("noValue");
  if (source === "measured") return tr("measured");
  if (source === "previous") return tr("fromEarlier");
  if (source === "user") return tr("editedByYou");
  return tr("fromSource", { source: SOURCE_NAMES[source] ?? nameOf(source) });
}

/** The name of a source for "Use … value". */
export function sourceName(source: string | undefined, nameOf: (id: string) => string): string {
  if (source === undefined) return tr("automatic");
  if (source === "measured") return tr("measured");
  if (source === "previous") return tr("earlier");
  return SOURCE_NAMES[source] ?? nameOf(source);
}

/** A copy of metadata (the dialog edits copies). */
export function cloneMeta(meta: GameMetadata): GameMetadata {
  return structuredClone(meta);
}

/** The values the dialog starts with: the user's for edited fields, else the automatic ones. */
export function startValues(info: GameEditInfo): GameMetadata {
  const meta = cloneMeta(info.automatic);
  for (const field of info.edits.edited) copyField(field, info.edits.meta, meta);
  return meta;
}

/** Copies one field between metadata objects. */
export function copyField(field: MetadataField, from: GameMetadata, into: GameMetadata): void {
  // Every field key is a property of GameMetadata with the same name.
  const value = from[field];
  Object.assign(into, { [field]: Array.isArray(value) ? [...value] : structuredClone(value) });
}

/** The edits to save: the edited fields' values, the user's links and the hidden ones. */
export function buildEdits(
  values: GameMetadata,
  edited: ReadonlySet<MetadataField>,
  ownLinks: GameLink[],
  hiddenLinks: string[],
): UserEdits {
  const meta = cloneMeta(values);
  meta.links = ownLinks.map((l) => ({ name: l.name.trim(), url: l.url.trim() }));
  return { meta, edited: [...edited].toSorted(), hiddenLinks };
}

/** How many of a section's fields the user edited. */
export function editedCount(section: MetadataField[], edited: ReadonlySet<MetadataField>): number {
  return section.filter((f) => edited.has(f)).length;
}

/** Hours as typed ("28", "28.5", "28h") to seconds; blank is `null`, junk is `undefined`. */
export function parseHours(text: string): number | null | undefined {
  const trimmed = text.trim().replace(/h$/i, "").trim().replace(",", ".");
  if (trimmed === "") return null;
  const hours = Number(trimmed);
  if (!Number.isFinite(hours) || hours < 0) return undefined;
  return Math.round(hours * 3600);
}

/** Seconds as hours for an input (one decimal at most). */
export function hoursText(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined) return "";
  return String(Math.round((seconds / 3600) * 10) / 10);
}

/** GB as typed to bytes (1024-based, like the shown sizes); blank is `null`, junk `undefined`. */
export function parseGigabytes(text: string): number | null | undefined {
  const trimmed = text
    .trim()
    .replace(/\s*gb$/i, "")
    .replace(",", ".");
  if (trimmed === "") return null;
  const gb = Number(trimmed);
  if (!Number.isFinite(gb) || gb < 0) return undefined;
  return Math.round(gb * 1024 ** 3);
}

export function gigabytesText(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "";
  return String(Math.round((bytes / 1024 ** 3) * 10) / 10);
}

/** A 0-100 score as typed; blank is `null`, junk `undefined`. */
export function parseScore(text: string): number | null | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  const value = Number(trimmed);
  if (!Number.isInteger(value) || value < 0 || value > 100) return undefined;
  return value;
}

/** A link address shown short: without scheme, `www.` and trailing slash. */
export function shortUrl(url: string): string {
  return url
    .trim()
    .replace(/^https?:\/\//i, "")
    .replace(/^www\./i, "")
    .replace(/\/$/, "");
}

/** Image files the dialog accepts, per kind. */
export const IMAGE_EXTENSIONS = ["png", "jpg", "jpeg", "webp"];
export const ICON_EXTENSIONS = ["png", "jpg", "jpeg", "ico", "exe", "dll"];

/** The web search a picker tab starts with. */
export function webQuery(name: string, slot: "cover" | "background" | "icon"): string {
  const word = { cover: "cover", background: "wallpaper", icon: "icon" }[slot];
  return `${name} ${word}`;
}

/** Moves an item one place up (-1) or down (+1); out of range leaves the list as it is. */
export function moveItem<T>(items: readonly T[], index: number, by: -1 | 1): T[] {
  const target = index + by;
  if (index < 0 || target < 0 || target >= items.length) return [...items];
  const copy = [...items];
  const [item] = copy.splice(index, 1);
  if (item !== undefined) copy.splice(target, 0, item);
  return copy;
}
