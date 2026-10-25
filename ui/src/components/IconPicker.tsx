import { For, onMount } from "solid-js";
import { isNavKey, nextIndex } from "../lib/gridNav";
import { Popover, type PopoverAnchor } from "./Popover";
import { VIEW_ICON_NAMES, ViewIcon } from "./ViewIcon";

const COLUMNS = 7;

/** A grid of the view icons; arrows move, Enter picks, Escape closes. */
export function IconPicker(props: {
  anchor: PopoverAnchor;
  selected: string;
  onPick: (name: string) => void;
  onClose: (returnFocus: boolean) => void;
}) {
  const buttons: HTMLButtonElement[] = [];

  onMount(() =>
    queueMicrotask(() => {
      const index = Math.max(0, VIEW_ICON_NAMES.indexOf(props.selected));
      buttons[index]?.focus();
    }),
  );

  function onKeyDown(event: KeyboardEvent) {
    const current = buttons.findIndex((b) => b === document.activeElement);
    if (!isNavKey(event.key) || event.key === "PageUp" || event.key === "PageDown") return;
    const next = nextIndex(current, event.key, COLUMNS, VIEW_ICON_NAMES.length, 1);
    if (next >= 0 && next !== current) {
      event.preventDefault();
      buttons[next]?.focus();
    }
  }

  return (
    <Popover
      anchor={props.anchor}
      class="icon-picker"
      role="listbox"
      label="Icon"
      onClose={props.onClose}
      onKeyDown={onKeyDown}
    >
      <For each={VIEW_ICON_NAMES}>
        {(name, index) => (
          <button
            ref={(el) => (buttons[index()] = el)}
            type="button"
            role="option"
            class="icon-picker__icon"
            classList={{ "icon-picker__icon--on": name === props.selected }}
            aria-selected={name === props.selected}
            aria-label={name.replace(/-\d$/, "").replaceAll("-", " ")}
            title={name.replace(/-\d$/, "").replaceAll("-", " ")}
            tabIndex={name === props.selected ? 0 : -1}
            onClick={() => {
              props.onClose(true);
              props.onPick(name);
            }}
          >
            <ViewIcon name={name} />
          </button>
        )}
      </For>
    </Popover>
  );
}
