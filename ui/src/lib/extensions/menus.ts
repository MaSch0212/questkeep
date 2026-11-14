import type { JSX } from "solid-js";
import type { MenuEntry } from "../../components/Menu";
import { guard, menuItemsFor } from "./registry";
import type { MenuContext, MenuItem, MenuLocation } from "./types";

/** Draws an extension's SVG markup as a menu icon. */
export type IconFactory = (svg: string) => () => JSX.Element;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null;

/**
 * An extension's menu items as the app's menu entries. Entries that are not a usable item are
 * left out (extensions are plain JavaScript), as are separators at the start, at the end and
 * after another separator. A throwing `onSelect` is logged.
 */
export function convertMenuItems(
  pluginId: string,
  items: readonly MenuItem[],
  icon: IconFactory,
): MenuEntry[] {
  const entries: MenuEntry[] = [];
  for (const item of Array.isArray(items) ? (items as readonly unknown[]) : []) {
    if (!isRecord(item)) continue;
    if (item.separator === true) {
      if (entries.length > 0 && entries.at(-1)?.kind !== "separator") {
        entries.push({ kind: "separator" });
      }
      continue;
    }
    if (typeof item.label !== "string" || item.label === "") continue;
    const drawn = typeof item.icon === "string" && item.icon !== "" ? icon(item.icon) : undefined;
    if (Array.isArray(item.items)) {
      const children = item.items as MenuItem[];
      entries.push({
        kind: "submenu",
        label: item.label,
        icon: drawn,
        entries: () => convertMenuItems(pluginId, children, icon),
      });
      continue;
    }
    const { label, onSelect } = item;
    if (typeof onSelect !== "function") continue;
    entries.push({
      label,
      icon: drawn,
      hint: typeof item.hint === "string" ? item.hint : undefined,
      danger: item.danger === true,
      disabled: item.disabled === true,
      checked: typeof item.checked === "boolean" ? item.checked : undefined,
      onSelect: () =>
        void guard(pluginId, `menu item "${label}" failed`, () => onSelect.call(item)),
    });
  }
  if (entries.at(-1)?.kind === "separator") entries.pop();
  return entries;
}

/**
 * What extensions add to one of the app's menus: each plugin's items after a separator; empty
 * when no extension adds anything.
 */
export function extensionMenuEntries(
  location: MenuLocation,
  context: MenuContext,
  icon: IconFactory,
): MenuEntry[] {
  const all: MenuEntry[] = [];
  for (const { plugin, items } of menuItemsFor(location, context)) {
    const entries = convertMenuItems(plugin, items, icon);
    if (entries.length > 0) all.push({ kind: "separator" }, ...entries);
  }
  return all;
}
