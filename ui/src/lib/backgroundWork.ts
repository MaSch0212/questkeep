import type { WorkQueue } from "../bindings/WorkQueue";

/** Jobs left in each background download queue, from `backgroundWork` core events. */
export type BackgroundWork = Record<WorkQueue, number>;

export const IDLE_WORK: BackgroundWork = { covers: 0, details: 0 };

export function isWorkBusy(work: BackgroundWork): boolean {
  return work.covers > 0 || work.details > 0;
}

const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** "Downloading 12 covers and details of 3 games…", or null while nothing is queued. */
export function workLabel(work: BackgroundWork): string | null {
  const covers = work.covers > 0 ? count(work.covers, "cover", "covers") : null;
  const details = work.details > 0 ? `details of ${count(work.details, "game", "games")}` : null;
  if (covers === null && details === null) return null;
  return `Downloading ${[covers, details].filter((part) => part !== null).join(" and ")}…`;
}
