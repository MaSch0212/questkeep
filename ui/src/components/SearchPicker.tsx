import Check from "lucide-solid/icons/check";
import Search from "lucide-solid/icons/search";
import { createMemo, createSignal, For, onMount, Show, type JSX } from "solid-js";
import { Popover, type PopoverAnchor } from "./Popover";

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
  footer?: JSX.Element;
}) {
  const [text, setText] = createSignal("");
  const [active, setActive] = createSignal(0);
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
    if (!item) return;
    if (!props.keepOpen) props.onClose(true);
    props.onPick(item.id);
  }

  return (
    <Popover
      anchor={props.anchor}
      class="picker"
      role="dialog"
      label={props.label}
      onClose={props.onClose}
      onKeyDown={(event) => {
        if (event.key === "ArrowDown") move(1);
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
          fallback={<p class="picker__empty">{props.empty ?? "Nothing found"}</p>}
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
                }}
                aria-selected={item.checked ?? active() === index()}
                onPointerMove={() => setActive(index())}
                onPointerDown={(event) => event.preventDefault()}
                onClick={() => pick(item)}
              >
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
