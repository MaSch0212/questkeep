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

const isPluginUpdates = (entry: Notification): boolean => entry.action?.type === "pluginUpdates";

/** Whether a plugin-updates entry that was already seen is still there (updates are pending). */
const hasReadPluginUpdates = (list: readonly Notification[]): boolean =>
  list.some((entry) => entry.read && isPluginUpdates(entry));

/**
 * What the badge on the bell counts: the unread entries, plus one for pending plugin updates
 * that were already read (an unread plugin-updates entry is among the unread ones already).
 * The badge stays until the updates are installed or their entry is dismissed.
 */
export const badgeCount = (list: readonly Notification[]): number =>
  unreadCount(list) + (hasReadPluginUpdates(list) ? 1 : 0);

/** Whether the badge counts something that is not unread (the label must not say "unread"). */
export const badgeHasSeenEntries = (list: readonly Notification[]): boolean =>
  hasReadPluginUpdates(list);

/**
 * The badge on the bell: nothing when it counts nothing, "error" when an unread entry is an
 * error, "info" when only information and pending plugin updates are counted, else "neutral".
 */
export function badgeKind(list: readonly Notification[]): "error" | "info" | "neutral" | null {
  if (badgeCount(list) === 0) return null;
  const unread = list.filter((entry) => !entry.read);
  if (unread.some((entry) => entry.level === "error")) return "error";
  return unread.every((entry) => entry.level === "info") ? "info" : "neutral";
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

/** What the coloured dot of an entry means, for screen readers. */
export function levelLabel(level: Notification["level"]): string {
  switch (level) {
    case "error":
      return tr("errorLabel");
    case "warning":
      return tr("warningLabel");
    case "info":
      return tr("infoLabel");
  }
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
    case "pluginUpdates":
      return tr("pluginUpdates");
  }
}
