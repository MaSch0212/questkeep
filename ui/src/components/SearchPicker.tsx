import Check from "lucide-solid/icons/check";
import GripVertical from "lucide-solid/icons/grip-vertical";
import Search from "lucide-solid/icons/search";
import { createMemo, createSignal, For, onMount, Show, type JSX } from "solid-js";
import { translator } from "../i18n";
import { libraryCommon } from "../i18n/libraryCommon";
import { Popover, type PopoverAnchor } from "./Popover";

const tr = translator(libraryCommon);

export interface PickerItem {
  id: string;
  label: string;
  /** Small text at the end (a game count, "active"). */
  hint?: string;
  /** Heading of the section the item starts or belongs to; items keep their order. */
  section?: string;
  /** Shows a check box; `true` ticks it. */
  checked?: boolean;
  /** Shown in italics (the "No …" entry). */
  special?: boolean;
  /** Shown but not choosable. */
  disabled?: boolean;
  /** Can be dragged (or moved with Alt+arrows) among the other movable items. */
  movable?: boolean;
}

/**
 * A searchable list in a popover: typing filters, arrows move, Enter chooses. With `keepOpen`
 * choosing toggles items and the list stays open (value lists); otherwise it closes.
 */
export function SearchPicker(props: {
  anchor: PopoverAnchor;
  label: string;
  placeholder: string;
  items: PickerItem[];
  onPick: (id: string) => void;
  onClose: (returnFocus: boolean) => void;
  keepOpen?: boolean;
  /** Shown when nothing matches the search. */
  empty?: string;
  /** Moves a movable item to the place of another one (drag, Alt+Up/Down). */
  onMove?: (id: string, to: string) => void;
  footer?: JSX.Element;
}) {
  const [text, setText] = createSignal("");
  const [active, setActive] = createSignal(0);
  const [dragged, setDragged] = createSignal<string | null>(null);
  let input: HTMLInputElement | undefined;
  let list: HTMLDivElement | undefined;

  const shown = createMemo(() => {
    const needle = text().trim().toLocaleLowerCase();
    return needle === ""
      ? props.items
      : props.items.filter((item) => item.label.toLocaleLowerCase().includes(needle));
  });

  onMount(() => queueMicrotask(() => input?.focus()));

  function move(by: number) {
    const count = shown().length;
    if (count === 0) return;
    const next = (active() + by + count) % count;
    setActive(next);
    const options = list?.querySelectorAll<HTMLElement>("[role=option]");
    options?.[next]?.scrollIntoView({ block: "nearest" });
  }

  function pick(item: PickerItem | undefined) {
    if (!item || item.disabled) return;
    if (!props.keepOpen) props.onClose(true);
    props.onPick(item.id);
  }

  /** Alt+Up/Down: swaps the active movable item with its movable neighbour. */
  function moveActive(by: number) {
    const items = shown();
    const item = items[active()];
    const target = items[active() + by];
    if (!props.onMove || !item?.movable || !target?.movable) return;
    props.onMove(item.id, target.id);
    setActive(active() + by);
  }

  return (
    <Popover
      anchor={props.anchor}
      class="picker"
      role="dialog"
      label={props.label}
      onClose={props.onClose}
      onKeyDown={(event) => {
        if (event.altKey && event.key === "ArrowDown") moveActive(1);
        else if (event.altKey && event.key === "ArrowUp") moveActive(-1);
        else if (event.key === "ArrowDown") move(1);
        else if (event.key === "ArrowUp") move(-1);
        else if (event.key === "Enter") pick(shown()[active()]);
        else return;
        event.preventDefault();
      }}
    >
      <label class="picker__search">
        <Search class="icon" aria-hidden="true" />
        <input
          ref={(el) => (input = el)}
          type="text"
          value={text()}
          placeholder={props.placeholder}
          aria-label={props.placeholder}
          aria-controls="picker-list"
          onInput={(event) => {
            setText(event.currentTarget.value);
            setActive(0);
          }}
        />
      </label>
      <div
        ref={(el) => (list = el)}
        id="picker-list"
        class="picker__list"
        role="listbox"
        aria-label={props.label}
      >
        <For
          each={shown()}
          fallback={<p class="picker__empty">{props.empty ?? tr("nothingFound")}</p>}
        >
          {(item, index) => (
            <>
              <Show when={item.section && item.section !== shown()[index() - 1]?.section}>
                <div class="picker__section" aria-hidden="true">
                  {item.section}
                </div>
              </Show>
              <div
                role="option"
                class="picker__item"
                classList={{
                  "picker__item--active": active() === index(),
                  "picker__item--special": item.special,
                  "picker__item--disabled": item.disabled,
                  "picker__item--dragging": dragged() === item.id,
                }}
                aria-selected={item.checked ?? active() === index()}
                aria-disabled={item.disabled}
                draggable={item.movable === true && props.onMove !== undefined}
                onDragStart={(event) => {
                  setDragged(item.id);
                  event.dataTransfer?.setData("text/plain", item.id);
                }}
                onDragEnd={() => setDragged(null)}
                onDragOver={(event) => {
                  if (item.movable && dragged() !== null) event.preventDefault();
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  const from = dragged();
                  if (from !== null && from !== item.id) props.onMove?.(from, item.id);
                  setDragged(null);
                }}
                onPointerMove={() => setActive(index())}
                onPointerDown={(event) => event.preventDefault()}
                onClick={() => pick(item)}
              >
                <Show when={props.onMove}>
                  <span class="picker__grip" aria-hidden="true">
                    <Show when={item.movable}>
                      <GripVertical class="icon" />
                    </Show>
                  </span>
                </Show>
                <Show when={item.checked !== undefined}>
                  <span class="picker__check" classList={{ "picker__check--on": item.checked }}>
                    <Show when={item.checked}>
                      <Check class="icon" aria-hidden="true" />
                    </Show>
                  </span>
                </Show>
                <span class="picker__label">{item.label}</span>
                <Show when={item.hint}>
                  <span class="picker__hint">{item.hint}</span>
                </Show>
              </div>
            </>
          )}
        </For>
      </div>
      {props.footer}
    </Popover>
  );
}
