import type { Game } from "../bindings/Game";

/**
 * Ctrl+click: adds or removes `id`. Starting from a single selection (`set` empty), the game
 * selected so far (`current`) joins first, so Ctrl+click adds to it. Keeps selection order.
 */
export function toggleSelected(
  set: ReadonlySet<string>,
  current: string | null,
  id: string,
): Set<string> {
  const next = new Set(set);
  if (next.size === 0 && current !== null) next.add(current);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

/**
 * Shift+click / Shift+arrows: every game from `anchor` to `target` in the view's visual order
 * (`order` may repeat a game that shows in several groups; the first and last places count).
 * Without a usable anchor only `target` is selected.
 */
export function rangeSelection(
  order: readonly string[],
  anchor: string | null,
  target: string,
): string[] {
  const end = order.indexOf(target);
  const start = anchor === null ? -1 : order.indexOf(anchor);
  if (end < 0 || start < 0) return [target];
  return [...new Set(order.slice(Math.min(start, end), Math.max(start, end) + 1))];
}

/**
 * How many covers of a selection the multi-select panel fans out and how far apart: at most
 * `max`, as many as fit `width` with covers `cover` wide overlapping by at least `minStep`.
 */
export function coverFan(
  count: number,
  width: number,
  cover: number,
  minStep: number,
  max = 8,
): { shown: number; step: number } {
  if (count <= 0) return { shown: 0, step: 0 };
  const room = Math.max(0, width - cover);
  const fits = Math.floor(room / minStep) + 1;
  const shown = Math.max(1, Math.min(count, max, fits));
  const step = shown > 1 ? Math.min(cover * 0.6, room / (shown - 1)) : 0;
  return { shown, step };
}

/** The value all games share, or "mixed" when they differ (undefined without games). */
export function common<T>(
  games: readonly Game[],
  value: (game: Game) => T,
): T | "mixed" | undefined {
  if (games.length === 0) return undefined;
  const first = value(games[0] as Game);
  return games.every((g) => value(g) === first) ? first : "mixed";
}

/** Per category: whether all, some or none of the games have it. */
export function categoryCoverage(
  games: readonly Game[],
  categoryId: string,
): "all" | "some" | "none" {
  const count = games.filter((g) => g.categories.includes(categoryId)).length;
  return count === 0 ? "none" : count === games.length ? "all" : "some";
}

/** "Steam 3 · Epic Games 2", most first, using each entry's stores. */
export function storeCounts(games: readonly Game[], name: (source: string) => string): string {
  const counts = new Map<string, number>();
  for (const game of games) {
    const sources = new Set([game.source, ...game.copies.map((c) => c.source)]);
    for (const source of sources) counts.set(source, (counts.get(source) ?? 0) + 1);
  }
  return [...counts.entries()]
    .toSorted((a, b) => b[1] - a[1] || name(a[0]).localeCompare(name(b[0])))
    .map(([source, n]) => `${name(source)} ${n}`)
    .join(" · ");
}
