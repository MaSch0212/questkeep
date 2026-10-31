import Pin from "lucide-solid/icons/pin";
import PinOff from "lucide-solid/icons/pin-off";
import X from "lucide-solid/icons/x";
import { Show } from "solid-js";
import { store } from "../lib/store";

/**
 * How the details panel sits: "docked" beside the list (always shown), "floating" over it by
 * choice, or null when it floats only because the window is too narrow (nothing to toggle).
 */
export type PanelPin = "docked" | "floating" | null;

/** The details panel's top bar: dock/unpin, and close while it floats. */
export function PanelHeader(props: { pin: PanelPin; onClose: () => void }) {
  return (
    <div class="details__header">
      <Show when={props.pin}>
        {(pin) => (
          <button
            type="button"
            class="icon-btn"
            aria-label={pin() === "docked" ? "Unpin panel" : "Dock panel"}
            title={
              pin() === "docked"
                ? "Unpin: show details over the list only while a game is selected"
                : "Dock: always show details beside the list"
            }
            onClick={() => store.setPanelFloating(pin() === "docked")}
          >
            <Show when={pin() === "docked"} fallback={<Pin class="icon" aria-hidden="true" />}>
              <PinOff class="icon" aria-hidden="true" />
            </Show>
          </button>
        )}
      </Show>
      <Show when={props.pin !== "docked"}>
        <button
          type="button"
          class="icon-btn"
          aria-label="Close details"
          title="Close (Esc)"
          onClick={() => props.onClose()}
        >
          <X class="icon" aria-hidden="true" />
        </button>
      </Show>
    </div>
  );
}

/** The docked panel while no game is selected. */
export function EmptyDetails() {
  return (
    <aside class="details details--split details--empty" aria-label="Game details">
      <PanelHeader pin="docked" onClose={() => undefined} />
      <p class="details__empty muted">Select a game to see its details here.</p>
    </aside>
  );
}
