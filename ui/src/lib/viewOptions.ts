import type { GroupBy } from "../bindings/GroupBy";
import type { SortField } from "../bindings/SortField";
import { locale, translator } from "../i18n";
import { libraryViewOptions } from "../i18n/libraryViewOptions";

const tr = translator(libraryViewOptions);

type Key = keyof typeof libraryViewOptions.en;

export interface Option<T> {
  id: T;
  /** Read at use time, so it follows the language. */
  label: string;
}

const option = <T>(id: T, key: Key): Option<T> => ({
  id,
  get label() {
    return tr(key);
  },
});

/**
 * A label inside a sentence ("grouped by store"): lowercase, except in German, where nouns keep
 * their capital.
 */
export function inSentence(label: string): string {
  return locale() === "de" ? label : label.toLocaleLowerCase(locale());
}

/** The common sort fields; the rest are under "More". */
export const SORTS: Option<SortField>[] = [
  option("name", "sortName"),
  option("lastPlayed", "sortLastPlayed"),
  option("playtime", "sortPlaytime"),
  option("dateAdded", "sortDateAdded"),
  option("releaseDate", "sortReleaseDate"),
];

export const MORE_SORTS: Option<SortField>[] = [
  option("playCount", "sortPlayCount"),
  option("criticScore", "sortCriticScore"),
  option("communityScore", "sortCommunityScore"),
  option("userScore", "sortUserScore"),
  option("timeToBeat", "sortTimeToBeat"),
  option("installSize", "sortInstallSize"),
  option("completionStatus", "sortCompletionStatus"),
  option("developer", "sortDeveloper"),
  option("publisher", "sortPublisher"),
  option("series", "sortSeries"),
];

/** The common groupings; the rest are in two submenus. */
export const GROUPS: Option<GroupBy>[] = [
  option("none", "groupNone"),
  option("source", "groupSource"),
  option("category", "groupCategory"),
  option("genre", "groupGenre"),
  option("completionStatus", "groupCompletionStatus"),
];

export const GAME_GROUPS: Option<GroupBy>[] = [
  option("tag", "groupTag"),
  option("feature", "groupFeature"),
  option("series", "groupSeries"),
  option("platform", "groupPlatform"),
  option("ageRating", "groupAgeRating"),
  option("developer", "groupDeveloper"),
  option("publisher", "groupPublisher"),
  option("gameMode", "groupGameMode"),
  option("theme", "groupTheme"),
  option("perspective", "groupPerspective"),
];

export const NUMBER_GROUPS: Option<GroupBy>[] = [
  option("releaseYear", "groupReleaseYear"),
  option("playtime", "groupPlaytime"),
  option("lastPlayed", "groupLastPlayed"),
  option("installSize", "groupInstallSize"),
  option("criticScore", "groupCriticScore"),
];

const ALL_SORTS = [...SORTS, ...MORE_SORTS];
const ALL_GROUPS = [...GROUPS, ...GAME_GROUPS, ...NUMBER_GROUPS];

export const sortLabel = (id: SortField) => ALL_SORTS.find((s) => s.id === id)?.label ?? id;
export const groupLabel = (id: GroupBy) => ALL_GROUPS.find((g) => g.id === id)?.label ?? id;
