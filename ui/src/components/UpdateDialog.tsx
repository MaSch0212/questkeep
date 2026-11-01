import Download from "lucide-solid/icons/download";
import { createSignal, Show } from "solid-js";
import type { UpdateInfo } from "../lib/api";
import * as api from "../lib/api";
import { translator } from "../i18n";
import { shellDialogs } from "../i18n/shellDialogs";
import { errorMessage } from "../lib/errors";
import { store } from "../lib/store";
import { Modal } from "./Modal";

const tr = translator(shellDialogs);

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
    <Modal title={tr("updateTitle")} onClose={props.onClose}>
      <div class="form">
        <p>
          {tr("updateMessage", {
            version: props.update.version,
            current: store.appInfo()?.version ?? null,
          })}
        </p>
        <Show when={props.update.notes}>{(notes) => <p class="update-notes">{notes()}</p>}</Show>
        <Show when={error()}>
          <p class="error-text" role="alert">
            {error()}
          </p>
        </Show>
        <div class="form__actions">
          <button type="button" class="btn" onClick={props.onClose}>
            {tr("later")}
          </button>
          <button
            type="button"
            class="btn btn--primary"
            data-autofocus
            disabled={installing()}
            onClick={() => void install()}
          >
            <Download class="icon" aria-hidden="true" />{" "}
            {installing() ? tr("installing") : tr("installAndRestart")}
          </button>
        </div>
      </div>
    </Modal>
  );
}
