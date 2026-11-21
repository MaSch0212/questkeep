/** One thing the settings search can find: a page or a single setting. */
export interface SearchEntry {
  /** Unique, e.g. `page:general` or `general.scanOnStartup`. */
  id: string;
  /** The page that shows it (a page id of the settings). */
  page: string;
  /** Its part of a multi-part page, when it is in one. */
  part?: string;
  /** The text the user sees for it (what the page is scrolled to). */
  label: string;
  /** Other words that find it. */
  keywords?: string;
}

/** Lower case, without accents, single spaces: what searches compare. */
export function normalize(text: string): string {
  return text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase().replace(/\s+/g, " ").trim();
}

/**
 * The entries that contain every word of `query` (in the label or the keywords), best first:
 * label starts with the query, then a label word does, then any other match; ties keep the
 * list's order. An empty query finds nothing.
 */
export function searchEntries(
  entries: readonly SearchEntry[],
  query: string,
  limit = 12,
): SearchEntry[] {
  const words = normalize(query).split(" ").filter(Boolean);
  if (words.length === 0) return [];
  const scored: { entry: SearchEntry; score: number; index: number }[] = [];
  entries.forEach((entry, index) => {
    const label = normalize(entry.label);
    const haystack = `${label} ${normalize(entry.keywords ?? "")}`;
    if (!words.every((w) => haystack.includes(w))) return;
    const labelWords = label.split(" ");
    let score = 2;
    if (label.startsWith(words.join(" "))) score = 0;
    else if (words.every((w) => labelWords.some((lw) => lw.startsWith(w)))) score = 1;
    scored.push({ entry, score, index });
  });
  scored.sort((a, b) => a.score - b.score || a.index - b.index);
  return scored.slice(0, limit).map((s) => s.entry);
}
