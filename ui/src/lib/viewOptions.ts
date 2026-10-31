import type { GroupBy } from "../bindings/GroupBy";
import type { SortField } from "../bindings/SortField";

export interface Option<T> {
  id: T;
  label: string;
}

/** The common sort fields; the rest are under "More". */
export const SORTS: Option<SortField>[] = [
  { id: "name", label: "Name" },
  { id: "lastPlayed", label: "Last played" },
  { id: "playtime", label: "Playtime" },
  { id: "dateAdded", label: "Date added" },
  { id: "releaseDate", label: "Release date" },
];

export const MORE_SORTS: Option<SortField>[] = [
  { id: "playCount", label: "Play count" },
  { id: "criticScore", label: "Critic score" },
  { id: "communityScore", label: "Community score" },
  { id: "userScore", label: "Your score" },
  { id: "timeToBeat", label: "Time to beat" },
  { id: "installSize", label: "Install size" },
  { id: "completionStatus", label: "Completion status" },
  { id: "developer", label: "Developer" },
  { id: "publisher", label: "Publisher" },
  { id: "series", label: "Series" },
];

/** The common groupings; the rest are in two submenus. */
export const GROUPS: Option<GroupBy>[] = [
  { id: "none", label: "Don't group" },
  { id: "source", label: "Store" },
  { id: "category", label: "Category" },
  { id: "genre", label: "Genre" },
  { id: "completionStatus", label: "Completion status" },
];

export const GAME_GROUPS: Option<GroupBy>[] = [
  { id: "tag", label: "Tag" },
  { id: "feature", label: "Feature" },
  { id: "series", label: "Series" },
  { id: "platform", label: "Platform" },
  { id: "ageRating", label: "Age rating" },
  { id: "developer", label: "Developer" },
  { id: "publisher", label: "Publisher" },
  { id: "gameMode", label: "Game mode" },
  { id: "theme", label: "Theme" },
  { id: "perspective", label: "Perspective" },
];

export const NUMBER_GROUPS: Option<GroupBy>[] = [
  { id: "releaseYear", label: "Release year" },
  { id: "playtime", label: "Playtime" },
  { id: "lastPlayed", label: "Last played" },
  { id: "installSize", label: "Install size" },
  { id: "criticScore", label: "Critic score" },
];

const ALL_SORTS = [...SORTS, ...MORE_SORTS];
const ALL_GROUPS = [...GROUPS, ...GAME_GROUPS, ...NUMBER_GROUPS];

export const sortLabel = (id: SortField) => ALL_SORTS.find((s) => s.id === id)?.label ?? id;
export const groupLabel = (id: GroupBy) => ALL_GROUPS.find((g) => g.id === id)?.label ?? id;
