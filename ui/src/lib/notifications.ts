import type { Notification } from "../bindings/Notification";
import type { NotificationAction } from "../bindings/NotificationAction";
import type { NotificationSource } from "../bindings/NotificationSource";
import { translator } from "../i18n";
import { shellNotifications } from "../i18n/shellNotifications";
import { formatClockTime, formatRelativeDate } from "./format";

const tr = translator(shellNotifications);

/** How many entries have not been seen in the bell yet. */
export const unreadCount = (list: readonly Notification[]): number =>
  list.filter((entry) => !entry.read).length;

/** The badge on the bell: nothing without unread entries, "error" when one of them is an error. */
export function badgeKind(list: readonly Notification[]): "error" | "neutral" | null {
  const unread = list.filter((entry) => !entry.read);
  if (unread.length === 0) return null;
  return unread.some((entry) => entry.level === "error") ? "error" : "neutral";
}

/** The badge text; large counts are cut off. */
export const badgeText = (count: number): string => (count > 99 ? "99+" : String(count));

/** "Today, 20:14" / "Yesterday, 22:41" / a date and the time. */
export function notificationTime(iso: string, now: Date = new Date()): string {
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return "";
  return `${formatRelativeDate(iso, now)}, ${formatClockTime(ms)}`;
}

/** What the part of the app that raised an entry is called. */
export function sourceLabel(source: NotificationSource): string {
  return tr(`source_${source}`);
}

/** The label of an entry's action button. */
export function actionLabel(action: NotificationAction): string {
  switch (action.type) {
    case "openFile":
      return action.path.toLowerCase().endsWith(".log") ? tr("openLog") : tr("openFile");
    case "editScript":
      return tr("editScript");
    case "storeSettings":
      return tr("storeSettings");
  }
}
