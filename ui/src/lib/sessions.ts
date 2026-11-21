/** A timestamp as the value of a `datetime-local` input (local time, to the minute). */
const pad = (n: number) => String(n).padStart(2, "0");

export function toLocalInput(iso: string): string {
  const date = new Date(iso);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** The ISO timestamp of a `datetime-local` value, or null when it is empty or invalid. */
export function fromLocalInput(value: string): string | null {
  const date = new Date(value);
  return value === "" || Number.isNaN(date.getTime()) ? null : date.toISOString();
}

/** Whole seconds for a number of minutes typed by the user; null for anything but 0 or more. */
export function minutesToSeconds(value: string): number | null {
  const minutes = Number(value.replace(",", "."));
  return value.trim() === "" || !Number.isFinite(minutes) || minutes < 0
    ? null
    : Math.round(minutes * 60);
}

/** Minutes for an input (whole minutes, one decimal when the seconds don't fit). */
export function secondsToMinutes(seconds: number): string {
  return String(Math.round((seconds / 60) * 10) / 10);
}

/** How many pages `total` rows take (at least one, so an empty list still has a page 1). */
export function pageCount(total: number, size: number): number {
  return Math.max(1, Math.ceil(total / size));
}
