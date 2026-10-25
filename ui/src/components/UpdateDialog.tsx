import Download from "lucide-solid/icons/download";
import { createSignal, Show } from "solid-js";
import type { UpdateInfo } from "../lib/api";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { store } from "../lib/store";
import { Modal } from "./Modal";

/** Offers the update found after start: install it now (the app restarts) or later. */
export function UpdateDialog(props: { update: UpdateInfo; onClose: () => void }) {
  const [installing, setInstalling] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  async function install() {
    setError(null);
    setInstalling(true);
    try {
      await api.installUpdate();
    } catch (failure) {
      setError(errorMessage(failure));
    }
    setInstalling(false);
  }

  return (
    <Modal title="Update available" onClose={props.onClose}>
      <div class="form">
        <p>
          Questkeep {props.update.version} is available
          <Show when={store.appInfo()?.version}>{(current) => <> (you have {current()})</>}</Show>.
          Installing it restarts Questkeep.
        </p>
        <Show when={props.update.notes}>{(notes) => <p class="update-notes">{notes()}</p>}</Show>
        <Show when={error()}>
          <p class="error-text" role="alert">
            {error()}
          </p>
        </Show>
        <div class="form__actions">
          <button type="button" class="btn" onClick={props.onClose}>
            Later
          </button>
          <button
            type="button"
            class="btn btn--primary"
            data-autofocus
            disabled={installing()}
            onClick={() => void install()}
          >
            <Download class="icon" aria-hidden="true" />{" "}
            {installing() ? "Installing…" : "Install and restart"}
          </button>
        </div>
      </div>
    </Modal>
  );
}
