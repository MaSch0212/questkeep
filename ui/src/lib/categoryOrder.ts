/** Moves `id` so it lands before the item at `index` (counted with `id` still in place). */
export function moveBefore(ids: readonly string[], id: string, index: number): string[] {
  const from = ids.indexOf(id);
  if (from < 0) return [...ids];
  const rest = ids.filter((other) => other !== id);
  const target = index > from ? index - 1 : index;
  rest.splice(Math.max(0, Math.min(target, rest.length)), 0, id);
  return rest;
}

/** Keyboard reordering (Alt+Up/Down): one step up or down, staying inside the list. */
export function nudge(ids: readonly string[], id: string, direction: -1 | 1): string[] {
  const from = ids.indexOf(id);
  if (from < 0) return [...ids];
  return moveBefore(ids, id, direction < 0 ? from - 1 : from + 2);
}

/** Drop index for a pointer at `y`: before the first row whose middle is below it. */
export function dropIndexAt(middles: readonly number[], y: number): number {
  const index = middles.findIndex((middle) => y < middle);
  return index < 0 ? middles.length : index;
}
