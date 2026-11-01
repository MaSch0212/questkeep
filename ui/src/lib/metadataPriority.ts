import type { MetadataField } from "../bindings/MetadataField";
import type { MetadataPriority } from "../bindings/MetadataPriority";
import { translator } from "../i18n";
import { settingsKeys } from "../i18n/settingsKeys";

const tr = translator(settingsKeys);

/** The fields with their own source order, in settings order. */
export const PRIORITY_FIELDS: { id: MetadataField }[] = [
  { id: "description" },
  { id: "releaseDate" },
  { id: "developers" },
  { id: "publishers" },
  { id: "genres" },
  { id: "tags" },
  { id: "features" },
  { id: "series" },
  { id: "ageRatings" },
  { id: "platforms" },
  { id: "gameModes" },
  { id: "themes" },
  { id: "perspectives" },
  { id: "criticScore" },
  { id: "communityScore" },
  { id: "installSize" },
  { id: "timeToBeat" },
];

/** The settings label of a field, in the current language. */
export const priorityFieldLabel = (id: MetadataField): string => tr(`field_${id}`);

// Mirrors the core's `default_order` (model/metadata.rs).
const STEAM_FIRST: ReadonlySet<MetadataField> = new Set([
  "features",
  "criticScore",
  "communityScore",
]);

/** IGDB, Steam, HowLongToBeat (Steam first for features and scores), then the rest by id. */
export function defaultOrder(field: MetadataField | null, sources: readonly string[]): string[] {
  const head =
    field !== null && STEAM_FIRST.has(field)
      ? ["steam", "igdb", "hltb"]
      : ["igdb", "steam", "hltb"];
  const rest = sources.filter((s) => !head.includes(s)).toSorted();
  return [...head.filter((h) => sources.includes(h)), ...rest];
}

/** The order a field (or, with `null`, the global list) uses, completed with missing sources. */
export function effectiveOrder(
  priority: MetadataPriority,
  field: MetadataField | null,
  sources: readonly string[],
): string[] {
  const own = field === null ? undefined : priority.fields[field];
  let chosen: string[];
  if (own !== undefined && own.length > 0) chosen = own;
  else if (priority.order.length === 0) chosen = defaultOrder(field, sources);
  else chosen = priority.order;
  const known = chosen.filter((s) => sources.includes(s));
  return [...new Set([...known, ...defaultOrder(field, sources)])];
}

/** Whether a field has its own order. */
export const hasOwnOrder = (priority: MetadataPriority, field: MetadataField) =>
  (priority.fields[field]?.length ?? 0) > 0;

/** Moves one entry of an order by `by` places (clamped). */
export function moveSource(order: readonly string[], id: string, by: number): string[] {
  const from = order.indexOf(id);
  if (from < 0) return [...order];
  const to = Math.max(0, Math.min(order.length - 1, from + by));
  const next = order.filter((s) => s !== id);
  next.splice(to, 0, id);
  return next;
}

export function withGlobalOrder(priority: MetadataPriority, order: string[]): MetadataPriority {
  return { ...priority, order };
}

/** Gives a field its own order, or (with `null`) makes it follow the global one again. */
export function withFieldOrder(
  priority: MetadataPriority,
  field: MetadataField,
  order: string[] | null,
): MetadataPriority {
  const { [field]: _old, ...rest } = priority.fields;
  return { ...priority, fields: order === null ? rest : { ...rest, [field]: order } };
}
