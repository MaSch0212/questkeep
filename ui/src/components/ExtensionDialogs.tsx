import { For, Show } from "solid-js";
import { translator } from "../i18n";
import { extensions } from "../i18n/extensions";
import { convertMenuItems } from "../lib/extensions/menus";
import { closePopupMenu, dialogs, popupMenu, type PopupMenu } from "../lib/extensions/registry";
import { store } from "../lib/store";
import { extensionIcon } from "./ExtensionIcon";
import { ExtensionMount } from "./ExtensionMount";
import { Menu } from "./Menu";
import { Modal } from "./Modal";
import type { PopoverAnchor } from "./Popover";

const tr = translator(extensions);

function PopupMenuHost(props: { menu: PopupMenu }) {
  // Taken once: the menu stays as it was opened.
  const { plugin, anchor, items } = props.menu;
  const entries = convertMenuItems(plugin, items, extensionIcon);
  const element = anchor instanceof HTMLElement ? anchor : undefined;
  const at: PopoverAnchor = element
    ? { rect: element.getBoundingClientRect(), align: "start" }
    : { x: (anchor as { x: number }).x, y: (anchor as { y: number }).y };
  const name = () => store.plugins().find((p) => p.id === plugin)?.name ?? plugin;
  if (entries.length === 0) {
    queueMicrotask(closePopupMenu);
    return null;
  }
  return (
    <Menu
      anchor={at}
      label={tr("menuOf", { name: name() })}
      entries={entries}
      inside={() => [element]}
      onClose={(returnFocus) => {
        closePopupMenu();
        if (returnFocus) element?.focus();
      }}
    />
  );
}

/** The dialogs and popup menus extensions opened (`ui.showDialog`, `ui.showMenu`). */
export function ExtensionDialogs() {
  return (
    <>
      <For each={dialogs()}>
        {(entry) => (
          <Modal
            title={entry.dialog.title}
            wide={entry.dialog.width === "large"}
            narrow={entry.dialog.width === "small"}
            onClose={entry.close}
          >
            <ExtensionMount
              plugin={entry.plugin}
              render={(container) => entry.dialog.render(container, entry.close)}
            />
          </Modal>
        )}
      </For>
      <Show when={popupMenu()} keyed>
        {(menu) => <PopupMenuHost menu={menu} />}
      </Show>
    </>
  );
}
