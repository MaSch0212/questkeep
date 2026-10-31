import type { MetadataField } from "../bindings/MetadataField";
import type { MetadataPriority } from "../bindings/MetadataPriority";

/** The fields with their own source order, in settings order. */
export const PRIORITY_FIELDS: { id: MetadataField; label: string }[] = [
  { id: "description", label: "Description" },
  { id: "releaseDate", label: "Release date" },
  { id: "developers", label: "Developers" },
  { id: "publishers", label: "Publishers" },
  { id: "genres", label: "Genres" },
  { id: "tags", label: "Tags" },
  { id: "features", label: "Features" },
  { id: "series", label: "Series" },
  { id: "ageRatings", label: "Age ratings" },
  { id: "platforms", label: "Platforms" },
  { id: "gameModes", label: "Game modes" },
  { id: "themes", label: "Themes" },
  { id: "perspectives", label: "Perspectives" },
  { id: "criticScore", label: "Critic score" },
  { id: "communityScore", label: "Community score" },
  { id: "installSize", label: "Install size" },
  { id: "timeToBeat", label: "Time to beat" },
];

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
