import type { QuickLaunchItem } from "../bindings/QuickLaunchItem";

/**
 * The tray popup's quick-launch strip as the settings show it: the user's arrangement, or every
 * installed store launcher (in store order) while there is none.
 */
export function quickLaunchItems(
  saved: readonly QuickLaunchItem[] | null,
  launchers: readonly string[],
): QuickLaunchItem[] {
  return saved ? [...saved] : launchers.map((id) => ({ kind: "store", id }));
}

/** A stable key for an entry (`store:steam`, `game:<uuid>`). */
export function quickKey(item: QuickLaunchItem): string {
  return `${item.kind}:${item.id}`;
}

/** The entry for a key made by {@link quickKey}; `null` for anything else. */
export function parseQuickKey(key: string): QuickLaunchItem | null {
  const at = key.indexOf(":");
  const kind = key.slice(0, at);
  const id = key.slice(at + 1);
  if (at < 0 || id === "" || (kind !== "store" && kind !== "game")) return null;
  return { kind, id };
}

/** `items` with `item` added at the end, unless it is there already. */
export function withQuickItem(
  items: readonly QuickLaunchItem[],
  item: QuickLaunchItem,
): QuickLaunchItem[] {
  return items.some((i) => quickKey(i) === quickKey(item)) ? [...items] : [...items, item];
}
