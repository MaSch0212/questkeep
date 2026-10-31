import X from "lucide-solid/icons/x";
import { createSignal, createUniqueId, For } from "solid-js";

/**
 * A list of values as chips with a text box: Enter or comma adds the typed value (suggestions
 * come from a datalist), × or Backspace in the empty box removes one.
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
  let input: HTMLInputElement | undefined;

  function add() {
    const value = text().trim().replace(/,$/, "").trim();
    setText("");
    if (value === "") return;
    const key = value.toLocaleLowerCase();
    if (props.values.some((v) => v.toLocaleLowerCase() === key)) return;
    props.onChange([...props.values, value]);
  }

  function remove(index: number) {
    props.onChange(props.values.filter((_, i) => i !== index));
    input?.focus();
  }

  return (
    <div
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
        list={listId}
        aria-label={props.label}
        aria-describedby={props.describedBy}
        placeholder="Add…"
        value={text()}
        onInput={(event) => {
          const value = event.currentTarget.value;
          setText(value);
          // Picking a suggestion from the list adds it at once.
          if (event.inputType === "insertReplacementText" && props.suggestions.includes(value)) {
            add();
          }
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === ",") {
            event.preventDefault();
            add();
          } else if (event.key === "Backspace" && text() === "" && props.values.length > 0) {
            event.preventDefault();
            remove(props.values.length - 1);
          }
        }}
        onBlur={add}
      />
      <datalist id={listId}>
        <For each={props.suggestions}>{(value) => <option value={value} />}</For>
      </datalist>
    </div>
  );
}
