import type { GameQuery } from "../bindings/GameQuery";
import type { ProviderInfo } from "../bindings/ProviderInfo";
import type { SortField } from "../bindings/SortField";

export type LibraryView = "all" | "favorites" | "hidden";

export function defaultQuery(): GameQuery {
  return {
    search: null,
    sort: "name",
    descending: false,
    sources: [],
    favoritesOnly: false,
    hidden: "visible",
    installedOnly: false,
  };
}

export function libraryView(query: GameQuery): LibraryView {
  if (query.hidden === "hidden") return "hidden";
  return query.favoritesOnly ? "favorites" : "all";
}

export function withLibraryView(query: GameQuery, view: LibraryView): GameQuery {
  return {
    ...query,
    favoritesOnly: view === "favorites",
    hidden: view === "hidden" ? "hidden" : "visible",
  };
}

/** Name sorts ascending by default; the numeric/date fields sort with the biggest first. */
export function defaultDescending(sort: SortField): boolean {
  return sort !== "name";
}

export function toggleSource(sources: readonly string[], id: string): string[] {
  return sources.includes(id) ? sources.filter((s) => s !== id) : [...sources, id];
}

export function normalizeSearch(value: string): string | null {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export interface SourceEntry {
  id: string;
  name: string;
}

/** Providers first (in their order), then any other source seen in the library, e.g. "fake". */
export function mergeSources(
  providers: readonly ProviderInfo[],
  seen: ReadonlySet<string>,
): SourceEntry[] {
  const entries: SourceEntry[] = providers.map((p) => ({ id: p.id, name: p.name }));
  const known = new Set(providers.map((p) => p.id));
  for (const id of [...seen].toSorted()) {
    if (!known.has(id)) entries.push({ id, name: id.charAt(0).toUpperCase() + id.slice(1) });
  }
  return entries;
}

export function scanSummaryText(summary: {
  added: number;
  updated: number;
  uninstalled: number;
}): string {
  return `${summary.added} added, ${summary.updated} updated, ${summary.uninstalled} uninstalled`;
}
