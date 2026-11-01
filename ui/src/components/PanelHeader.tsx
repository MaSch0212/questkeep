import { translator } from "../i18n";
import { gameChrome } from "../i18n/gameChrome";
import Pin from "lucide-solid/icons/pin";
import PinOff from "lucide-solid/icons/pin-off";
import X from "lucide-solid/icons/x";
import { Show } from "solid-js";
import { store } from "../lib/store";

const tr = translator(gameChrome);

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
            aria-label={pin() === "docked" ? tr("unpinPanel") : tr("dockPanel")}
            title={pin() === "docked" ? tr("unpinTitle") : tr("dockTitle")}
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
          aria-label={tr("closeDetails")}
          title={tr("closeTitle")}
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
    <aside class="details details--split details--empty" aria-label={tr("gameDetails")}>
      <PanelHeader pin="docked" onClose={() => undefined} />
      <p class="details__empty muted">{tr("selectToSeeHere")}</p>
    </aside>
  );
}
