import { extensionMenuEntries, type IconFactory } from "../lib/extensions/menus";
import type { MenuContext, MenuLocation } from "../lib/extensions/types";
import type { MenuEntry } from "./Menu";

/** An extension's icon: SVG markup put into the page as is (extensions are trusted code). */
export function ExtensionIcon(props: { svg: string }) {
  return <span class="ext-icon" aria-hidden="true" innerHTML={props.svg} />;
}

export const extensionIcon: IconFactory = (svg) => () => <ExtensionIcon svg={svg} />;

/** What extensions add to one of the app's menus (each plugin's items after a separator). */
export const extensionEntries = (location: MenuLocation, context: MenuContext = {}): MenuEntry[] =>
  extensionMenuEntries(location, context, extensionIcon);
