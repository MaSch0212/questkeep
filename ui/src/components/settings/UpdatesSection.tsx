import Download from "lucide-solid/icons/download";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import { createSignal, Show } from "solid-js";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { store } from "../../lib/store";

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; update: api.UpdateInfo; confirming: boolean }
  | { kind: "installing"; update: api.UpdateInfo }
  | { kind: "error"; message: string };

export function UpdatesSection() {
  // The check after start may have found one already.
  const known = store.update();
  const [state, setState] = createSignal<State>(
    known ? { kind: "available", update: known, confirming: false } : { kind: "idle" },
  );

  async function check() {
    setState({ kind: "checking" });
    try {
      const update = await api.checkUpdate();
      store.setUpdate(update);
      setState(update ? { kind: "available", update, confirming: false } : { kind: "current" });
    } catch (error) {
      setState({ kind: "error", message: errorMessage(error) });
    }
  }

  async function install(update: api.UpdateInfo) {
    setState({ kind: "installing", update });
    try {
      await api.installUpdate();
    } catch (error) {
      setState({ kind: "error", message: errorMessage(error) });
    }
  }

  const available = () => {
    const s = state();
    return s.kind === "available" ? s : null;
  };
  const installing = () => {
    const s = state();
    return s.kind === "installing" ? s : null;
  };
  const shown = () => available()?.update ?? installing()?.update ?? null;

  return (
    <div class="form">
      <div class="scan-row">
        <button
          type="button"
          class="btn"
          disabled={state().kind === "checking" || state().kind === "installing"}
          onClick={() => void check()}
        >
          <RefreshCw class="icon" aria-hidden="true" />{" "}
          {state().kind === "checking" ? "Checking…" : "Check for updates"}
        </button>
        <Show when={state().kind === "current"}>
          <span role="status">You&apos;re up to date.</span>
        </Show>
      </div>
      <Show when={state()} keyed>
        {(s) => (
          <Show when={s.kind === "error" ? s.message : null}>
            {(message) => (
              <p class="error-text" role="alert">
                {message()}
              </p>
            )}
          </Show>
        )}
      </Show>
      <Show when={shown()}>
        {(update) => (
          <div class="form">
            <p role="status">Version {update().version} is available</p>
            <Show when={update().notes}>{(notes) => <p class="update-notes">{notes()}</p>}</Show>
            <Show
              when={installing() === null}
              fallback={
                <button type="button" class="btn btn--primary" disabled>
                  Installing…
                </button>
              }
            >
              <Show
                when={available()?.confirming}
                fallback={
                  <div>
                    <button
                      type="button"
                      class="btn btn--primary"
                      onClick={() =>
                        setState({ kind: "available", update: update(), confirming: true })
                      }
                    >
                      <Download class="icon" aria-hidden="true" /> Install and restart
                    </button>
                  </div>
                }
              >
                <p>Questkeep will download the update and restart. Continue?</p>
                <div class="details__row">
                  <button
                    type="button"
                    class="btn btn--primary"
                    onClick={() => void install(update())}
                  >
                    Install and restart now
                  </button>
                  <button
                    type="button"
                    class="btn"
                    onClick={() =>
                      setState({ kind: "available", update: update(), confirming: false })
                    }
                  >
                    Cancel
                  </button>
                </div>
              </Show>
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}
