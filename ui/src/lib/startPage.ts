import type { StartupPage } from "../bindings/StartupPage";

/** Where Questkeep opens: the start page or a view. */
export type StartTarget = { kind: "home" } | { kind: "view"; id: string | null };

/** The value stored as the page open last for the start page (views store their id). */
export const HOME_PAGE = "home";

/**
 * The page to open at startup. Falls back to the first view (`id: null`) when the start page is
 * off or the remembered or chosen view is gone.
 */
export function startupTarget(
  startup: StartupPage,
  showStartPage: boolean,
  lastPage: string | null,
  viewIds: readonly string[],
): StartTarget {
  const home: StartTarget = showStartPage ? { kind: "home" } : { kind: "view", id: null };
  const view = (id: string | null): StartTarget =>
    id !== null && viewIds.includes(id) ? { kind: "view", id } : { kind: "view", id: null };
  switch (startup.kind) {
    case "start":
      return home;
    case "view":
      return view(startup.id);
    case "last":
      if (lastPage === null || lastPage === HOME_PAGE) return home;
      return view(lastPage);
  }
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAY_MS = 24 * 60 * 60 * 1000;

function localMidnight(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

/**
 * A shelf badge date relative to `now` in local time: "Today", "Yesterday", "3 days ago",
 * "2 weeks ago", then "18 Sep" (with the year when it is not this year).
 */
export function relativeDay(iso: string, now: Date = new Date()): string {
  const date = new Date(iso);
  const days = Math.round((localMidnight(now) - localMidnight(date)) / DAY_MS);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return `${days} days ago`;
  if (days < 28) {
    const weeks = Math.floor(days / 7);
    return weeks === 1 ? "1 week ago" : `${weeks} weeks ago`;
  }
  const short = `${date.getDate()} ${MONTHS[date.getMonth()]}`;
  return date.getFullYear() === now.getFullYear() ? short : `${short} ${date.getFullYear()}`;
}

/** Hours with one decimal ("1.5"), "<0.1" for a few minutes, or "–" for none. */
export function hoursLabel(seconds: number): string {
  if (seconds <= 0) return "–";
  return seconds < 180 ? "<0.1" : (seconds / 3600).toFixed(1);
}

/** A short weekday name for a `YYYY-MM-DD` day, or "Today". */
export function dayLabel(day: string, now: Date = new Date()): string {
  const [y, m, d] = day.split("-").map(Number);
  const date = new Date(y ?? 0, (m ?? 1) - 1, d ?? 1);
  if (localMidnight(date) === localMidnight(now)) return "Today";
  return date.toLocaleDateString("en", { weekday: "short" });
}
