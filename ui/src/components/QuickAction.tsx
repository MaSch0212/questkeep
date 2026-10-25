import { Show } from "solid-js";
import type { Game } from "../bindings/Game";
import Download from "lucide-solid/icons/download";
import Play from "lucide-solid/icons/play";
import { installWithToast, launchWithToast } from "../lib/actions";
import { primaryAction } from "../lib/primaryAction";
import { store } from "../lib/store";

interface QuickActionProps {
  game: Game;
  /** Reachable with Tab; only the selected tile or row is, so Tab does not walk the whole page. */
  tabbable: boolean;
}

// The library's own key handler turns Enter into "open details" and would cancel the click.
function onKeyDown(event: KeyboardEvent) {
  if (event.key === "Enter" || event.key === " ") event.stopPropagation();
}

/**
 * The primary action of a tile or row: Play, Install, or the running state. A real button that
 * does not select the tile or open details (events stop here). Same code path as the double
 * click and Ctrl+Enter / F5 on the library: `activateGame`.
 */
export function QuickAction(props: QuickActionProps) {
  const action = () => primaryAction(props.game, store.isRunning(props.game.id));

  return (
    <Show when={action()}>
      {(kind) => (
        <button
          type="button"
          class="quick-action"
          classList={{ "quick-action--running": kind() === "running" }}
          tabindex={props.tabbable ? 0 : -1}
          disabled={kind() === "running"}
          aria-label={
            kind() === "running"
              ? `${props.game.name} is running`
              : `${kind() === "play" ? "Play" : "Install"} ${props.game.name}`
          }
          title={kind() === "play" ? "Play (Ctrl+Enter)" : kind() === "install" ? "Install" : ""}
          onClick={(event) => {
            event.stopPropagation();
            if (kind() === "install") void installWithToast(props.game.id);
            else void launchWithToast(props.game.id);
          }}
          onDblClick={(event) => event.stopPropagation()}
          onKeyDown={onKeyDown}
        >
          <Show when={kind() === "install"}>
            <Download class="icon" aria-hidden="true" />
          </Show>
          <Show when={kind() === "play"}>
            <Play class="icon icon--filled" aria-hidden="true" />
          </Show>
          <span>{kind() === "play" ? "Play" : kind() === "install" ? "Install" : "Running"}</span>
        </button>
      )}
    </Show>
  );
}
