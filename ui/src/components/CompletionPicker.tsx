import { translator } from "../i18n";
import { gameChrome } from "../i18n/gameChrome";
import ChevronDown from "lucide-solid/icons/chevron-down";
import { createSignal, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { store } from "../lib/store";
import { Menu } from "./Menu";
import type { PopoverAnchor } from "./Popover";

const tr = translator(gameChrome);

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
        aria-label={tr("completionAria", { status: current()?.name ?? tr("none") })}
        aria-haspopup="menu"
        aria-expanded={anchor() !== null}
        onClick={() => {
          const rect = button?.getBoundingClientRect();
          setAnchor(anchor() || !rect ? null : { rect, align: "start" });
        }}
      >
        <span classList={{ "status-btn__none": !current() }}>
          {current()?.name ?? tr("noStatus")}
        </span>
        <ChevronDown class="icon" aria-hidden="true" />
      </button>
      <Show when={anchor()}>
        {(at) => (
          <Menu
            anchor={at()}
            label={tr("completionStatus")}
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
                label: tr("noStatus"),
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
