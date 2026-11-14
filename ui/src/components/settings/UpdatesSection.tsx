import Download from "lucide-solid/icons/download";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import { createSignal, Show } from "solid-js";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { translator } from "../../i18n";
import { settingsPage } from "../../i18n/settingsPage";
import { store } from "../../lib/store";

const tr = translator(settingsPage);

type State = "idle" | "checking" | "current";

/** Checks for a newer version; a found one (also by the check after start) opens the update
 * dialog, and the button then offers it directly. */
export function UpdatesSection() {
  const [state, setState] = createSignal<State>("idle");
  const [error, setError] = createSignal<string | null>(null);

  async function check() {
    setState("checking");
    setError(null);
    try {
      const update = await api.checkUpdate();
      store.setUpdate(update);
      setState(update ? "idle" : "current");
      if (update) store.setUpdateOpen(true);
    } catch (failure) {
      setState("idle");
      setError(errorMessage(failure));
    }
  }

  return (
    <div class="form">
      <div class="scan-row">
        <Show
          when={store.update()}
          fallback={
            <button
              type="button"
              class="btn"
              disabled={state() === "checking"}
              onClick={() => void check()}
            >
              <RefreshCw class="icon" aria-hidden="true" />
              {state() === "checking" ? tr("checking") : tr("check")}
            </button>
          }
        >
          {(update) => (
            <button
              type="button"
              class="btn btn--primary"
              onClick={() => store.setUpdateOpen(true)}
            >
              <Download class="icon" aria-hidden="true" />
              {tr("updateTo", { version: update().version })}
            </button>
          )}
        </Show>
        <Show when={state() === "current"}>
          <span role="status">{tr("upToDate")}</span>
        </Show>
      </div>
      <Show when={error()}>
        <p class="error-text" role="alert">
          {error()}
        </p>
      </Show>
    </div>
  );
}
