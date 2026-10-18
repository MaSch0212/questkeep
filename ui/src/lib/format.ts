/** "12h 5m", "45m", "Never played". */
export function formatPlaytime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "Never played";
  const totalMinutes = Math.floor(seconds / 60);
  if (totalMinutes < 1) return "Less than a minute";
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours === 0) return `${minutes}m`;
  return minutes === 0 ? `${hours}h` : `${hours}h ${minutes}m`;
}

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

export function formatDate(iso: string | null): string {
  if (!iso) return "Unknown";
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? "Unknown" : date.toLocaleDateString();
}

const RELEASE_RE = /^(\d{4})(?:-(\d{2})(?:-(\d{2}))?)?$/;

/** Release year of a `YYYY[-MM[-DD]]` string, or null when absent or malformed. */
export function releaseYear(value: string | null): string | null {
  const match = value ? RELEASE_RE.exec(value.trim()) : null;
  return match?.[1] ?? null;
}

/**
 * Formats `YYYY-MM-DD` as a long date, `YYYY-MM` as "Month YYYY" and `YYYY` as the bare year.
 * Anything unparseable is shown as-is so odd store data is not hidden.
 */
export function formatReleaseDate(value: string | null, locale?: string): string | null {
  const text = value?.trim();
  if (!text) return null;
  const match = RELEASE_RE.exec(text);
  if (!match) return text;
  const year = Number(match[1]);
  if (match[2] === undefined) return String(year);
  const month = Number(match[2]);
  if (month < 1 || month > 12) return String(year);
  // UTC on both sides so the date never shifts with the user's time zone.
  const date = new Date(Date.UTC(year, month - 1, match[3] === undefined ? 1 : Number(match[3])));
  const options: Intl.DateTimeFormatOptions =
    match[3] === undefined
      ? { year: "numeric", month: "long", timeZone: "UTC" }
      : { dateStyle: "long", timeZone: "UTC" };
  return new Intl.DateTimeFormat(locale, options).format(date);
}

/** "Today", "Yesterday", "3 days ago", otherwise the locale date. */
export function formatRelativeDate(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "Never";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "Unknown";
  const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days >= 2 && days <= 6) return `${days} days ago`;
  return date.toLocaleDateString();
}
