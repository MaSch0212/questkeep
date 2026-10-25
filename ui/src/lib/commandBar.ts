import type { Game } from "../bindings/Game";

/** How many results the command bar shows. */
export const COMMAND_RESULTS = 7;

/**
 * Orders name matches for the command bar: visible games before hidden ones, then names where
 * the match starts earlier, then by sorting name. Keeps at most {@link COMMAND_RESULTS}.
 */
export function rankMatches(games: readonly Game[], search: string): Game[] {
  const needle = search.trim().toLowerCase();
  const position = (game: Game) => {
    const at = game.name.toLowerCase().indexOf(needle);
    return at < 0 ? Number.MAX_SAFE_INTEGER : at;
  };
  return games
    .toSorted(
      (a, b) =>
        Number(a.hidden) - Number(b.hidden) ||
        position(a) - position(b) ||
        a.sortingName.localeCompare(b.sortingName),
    )
    .slice(0, COMMAND_RESULTS);
}

/** A name split around the first case-insensitive match, for highlighting. */
export function splitMatch(
  name: string,
  search: string,
): { before: string; match: string; after: string } {
  const needle = search.trim();
  const at = needle === "" ? -1 : name.toLowerCase().indexOf(needle.toLowerCase());
  if (at < 0) return { before: name, match: "", after: "" };
  return {
    before: name.slice(0, at),
    match: name.slice(at, at + needle.length),
    after: name.slice(at + needle.length),
  };
}
