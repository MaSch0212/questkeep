import ChevronDown from "lucide-solid/icons/chevron-down";
import { createSignal, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { store } from "../lib/store";
import { Menu } from "./Menu";
import type { PopoverAnchor } from "./Popover";

const statuses = () => store.completion().statuses;

/** A game's completion status as a button that opens the list of statuses. */
export function CompletionPicker(props: { game: Game }) {
  const [anchor, setAnchor] = createSignal<PopoverAnchor | null>(null);
  let button: HTMLButtonElement | undefined;
  const current = () => statuses().find((s) => s.id === props.game.completionStatus);
  const set = (id: string | null) => void store.setCompletionStatus([props.game.id], id);

  return (
    <>
      <button
        ref={(el) => (button = el)}
        type="button"
        class="status-btn"
        aria-label={`Completion status: ${current()?.name ?? "none"}`}
        aria-haspopup="menu"
        aria-expanded={anchor() !== null}
        onClick={() => {
          const rect = button?.getBoundingClientRect();
          setAnchor(anchor() || !rect ? null : { rect, align: "start" });
        }}
      >
        <span classList={{ "status-btn__none": !current() }}>{current()?.name ?? "No status"}</span>
        <ChevronDown class="icon" aria-hidden="true" />
      </button>
      <Show when={anchor()}>
        {(at) => (
          <Menu
            anchor={at()}
            label="Completion status"
            inside={() => [button]}
            onClose={(returnFocus) => {
              setAnchor(null);
              if (returnFocus) button?.focus();
            }}
            entries={[
              ...statuses().map((status) => ({
                label: status.name,
                checked: props.game.completionStatus === status.id,
                onSelect: () => set(status.id),
              })),
              { kind: "separator" as const },
              {
                label: "No status",
                checked: props.game.completionStatus === null,
                onSelect: () => set(null),
              },
            ]}
          />
        )}
      </Show>
    </>
  );
}
