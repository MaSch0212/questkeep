import X from "lucide-solid/icons/x";
import { createMemo, createSignal, createUniqueId, For, Show } from "solid-js";
import { Popover, type PopoverAnchor } from "./Popover";

/** Suggestions shown at most; typing narrows them down. */
const MAX_SUGGESTIONS = 50;

/**
 * A list of values as chips with a text box (a combobox): Enter or comma adds the typed value,
 * the list under the box offers matching suggestions (arrow keys and Enter, or a click), × or
 * Backspace in the empty box removes one.
 */
export function ChipInput(props: {
  label: string;
  values: string[];
  suggestions: string[];
  onChange: (values: string[]) => void;
  edited?: boolean;
  describedBy?: string;
  /** Id of the text box, for an outside <label>. */
  id?: string;
}) {
  const listId = createUniqueId();
  const [text, setText] = createSignal("");
  const [anchor, setAnchor] = createSignal<PopoverAnchor | null>(null);
  const [active, setActive] = createSignal(-1);
  /** The list is as wide as the box. */
  const [width, setWidth] = createSignal(0);
  let input: HTMLInputElement | undefined;
  let box: HTMLDivElement | undefined;

  const matches = createMemo(() => {
    const query = text().trim().toLocaleLowerCase();
    const taken = new Set(props.values.map((v) => v.toLocaleLowerCase()));
    return props.suggestions
      .filter((s) => !taken.has(s.toLocaleLowerCase()) && s.toLocaleLowerCase().includes(query))
      .slice(0, MAX_SUGGESTIONS);
  });
  const listShown = () => anchor() !== null && matches().length > 0;

  function open() {
    const rect = box?.getBoundingClientRect();
    if (!rect) return;
    setWidth(rect.width);
    setAnchor({ rect, align: "below" });
  }

  function close() {
    setAnchor(null);
    setActive(-1);
  }

  function add(raw = text()) {
    const value = raw.trim().replace(/,$/, "").trim();
    setText("");
    setActive(-1);
    if (value === "") return;
    const key = value.toLocaleLowerCase();
    if (props.values.some((v) => v.toLocaleLowerCase() === key)) return;
    props.onChange([...props.values, value]);
    // The box may have grown a line: the list moves with it.
    if (anchor()) queueMicrotask(open);
  }

  function remove(index: number) {
    props.onChange(props.values.filter((_, i) => i !== index));
    input?.focus();
  }

  function move(step: number) {
    if (!anchor()) open();
    const count = matches().length;
    if (count === 0) return;
    setActive((i) => (i + step + count) % count);
    document.getElementById(`${listId}-${active()}`)?.scrollIntoView?.({ block: "nearest" });
  }

  return (
    <div
      ref={(el) => (box = el)}
      class="chip-input"
      classList={{ "chip-input--edited": props.edited === true }}
      onClick={(event) => {
        if (event.target === event.currentTarget) input?.focus();
      }}
    >
      <For each={props.values}>
        {(value, index) => (
          <span class="chip chip-input__chip">
            {value}
            <button
              type="button"
              class="chip-input__remove"
              aria-label={`Remove ${value}`}
              onClick={() => remove(index())}
            >
              <X class="icon icon--xs" aria-hidden="true" />
            </button>
          </span>
        )}
      </For>
      <input
        ref={(el) => (input = el)}
        id={props.id}
        class="chip-input__text"
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={listShown()}
        aria-controls={listId}
        aria-activedescendant={active() >= 0 ? `${listId}-${active()}` : undefined}
        aria-label={props.label}
        aria-describedby={props.describedBy}
        placeholder="Add…"
        autocomplete="off"
        value={text()}
        onFocus={open}
        onInput={(event) => {
          setText(event.currentTarget.value);
          setActive(-1);
          if (!anchor()) open();
        }}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            move(event.key === "ArrowDown" ? 1 : -1);
          } else if (event.key === "Enter" || event.key === ",") {
            event.preventDefault();
            const picked = listShown() ? matches()[active()] : undefined;
            add(picked ?? text());
          } else if (event.key === "Escape" && listShown()) {
            event.preventDefault();
            event.stopPropagation();
            close();
          } else if (event.key === "Backspace" && text() === "" && props.values.length > 0) {
            event.preventDefault();
            remove(props.values.length - 1);
          }
        }}
        onBlur={() => {
          close();
          add();
        }}
      />
      {/* Keyed: a new anchor (the box grew) places the list again. */}
      <Show when={listShown() ? anchor() : null} keyed>
        {(at) => (
          <Popover anchor={at} class="chip-input__popover" inside={() => [box]} onClose={close}>
            <ul
              id={listId}
              class="chip-input__list"
              role="listbox"
              aria-label={props.label}
              style={{ width: `${width()}px` }}
            >
              <For each={matches()}>
                {(value, index) => (
                  <li
                    id={`${listId}-${index()}`}
                    class="chip-input__option"
                    classList={{ "chip-input__option--active": index() === active() }}
                    role="option"
                    aria-selected={index() === active()}
                    // Keeps the focus (and the typed text) in the box.
                    onMouseDown={(event) => event.preventDefault()}
                    onMouseEnter={() => setActive(index())}
                    onClick={() => add(value)}
                  >
                    {value}
                  </li>
                )}
              </For>
            </ul>
          </Popover>
        )}
      </Show>
    </div>
  );
}
