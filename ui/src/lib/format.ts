import { intlLocale, translator } from "../i18n";
import { format } from "../i18n/format";

const tr = translator(format);

/** "12h 5m", "45m", "Never played". */
export function formatPlaytime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return tr("neverPlayed");
  const totalMinutes = Math.floor(seconds / 60);
  if (totalMinutes < 1) return tr("lessThanMinute");
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours === 0) return tr("minutes", { m: minutes });
  return minutes === 0
    ? tr("hours", { h: hours.toLocaleString(intlLocale()) })
    : tr("hoursMinutes", { h: hours.toLocaleString(intlLocale()), m: minutes });
}

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

export function formatDate(iso: string | null): string {
  if (!iso) return tr("unknown");
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? tr("unknown") : date.toLocaleDateString(intlLocale());
}

/** Date and time of an ISO timestamp, in the local format ("Unknown" when unparseable). */
export function formatDateTime(iso: string | null): string {
  if (!iso) return tr("unknown");
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? tr("unknown") : date.toLocaleString(intlLocale());
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
  return new Intl.DateTimeFormat(locale ?? intlLocale(), options).format(date);
}

/** Clock time (hours and minutes) of a timestamp in milliseconds, in the user's locale. */
export function formatClockTime(ms: number, locale?: string): string {
  return new Date(ms).toLocaleTimeString(locale ?? intlLocale(), {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** "Today", "Yesterday", "3 days ago", otherwise the locale date. */
export function formatRelativeDate(iso: string | null, now: Date = new Date()): string {
  if (!iso) return tr("never");
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return tr("unknown");
  const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
  if (days === 0) return tr("today");
  if (days === 1) return tr("yesterday");
  if (days >= 2 && days <= 6) return tr("daysAgo", { n: days });
  return date.toLocaleDateString(intlLocale());
}

/** "Just now", "12 min ago", "3 h ago", then like {@link formatRelativeDate}. */
export function formatAgo(iso: string | null, now: Date = new Date()): string {
  if (!iso) return tr("never");
  const time = new Date(iso).getTime();
  if (Number.isNaN(time)) return tr("unknown");
  const minutes = Math.floor((now.getTime() - time) / 60_000);
  if (minutes < 1) return tr("justNow");
  if (minutes < 60) return tr("minutesAgo", { n: minutes });
  if (minutes < 12 * 60) return tr("hoursAgo", { n: Math.floor(minutes / 60) });
  return formatRelativeDate(iso, now);
}

/** "158.7 GB", "512 MB", "12 KB" (1024-based, like Explorer). */
export function formatSize(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit >= 3 ? 1 : 0;
  const number = value.toLocaleString(intlLocale(), {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
  return `${number} ${units[unit]}`;
}
