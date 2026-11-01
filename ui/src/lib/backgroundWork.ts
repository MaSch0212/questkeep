import type { WorkQueue } from "../bindings/WorkQueue";
import { translator } from "../i18n";
import { shellSync } from "../i18n/shellSync";

const tr = translator(shellSync);

/** Jobs left in each background download queue, from `backgroundWork` core events. */
export type BackgroundWork = Record<WorkQueue, number>;

export const IDLE_WORK: BackgroundWork = { covers: 0, details: 0 };

export function isWorkBusy(work: BackgroundWork): boolean {
  return work.covers > 0 || work.details > 0;
}

/** "Downloading 12 covers and details of 3 games…", or null while nothing is queued. */
export function workLabel(work: BackgroundWork): string | null {
  const covers = work.covers > 0 ? tr("covers", { count: work.covers }) : null;
  const details = work.details > 0 ? tr("detailsOf", { count: work.details }) : null;
  if (covers === null && details === null) return null;
  const what =
    covers !== null && details !== null
      ? tr("and", { a: covers, b: details })
      : (covers ?? details);
  return tr("downloading", { what: what ?? "" });
}

/** "Backing up 42%…" while a backup runs, else null. */
export function backupLabel(progress: { done: number; total: number } | null): string | null {
  if (progress === null) return null;
  if (progress.total === 0) return tr("backingUp");
  return tr("backingUpPercent", {
    percent: Math.floor((progress.done * 100) / progress.total),
  });
}
