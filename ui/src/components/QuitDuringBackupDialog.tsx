import { createSignal, Show } from "solid-js";
import * as api from "../lib/api";
import { translator } from "../i18n";
import { shellDialogs } from "../i18n/shellDialogs";
import { errorMessage } from "../lib/errors";
import { store } from "../lib/store";
import { Modal } from "./Modal";

const tr = translator(shellDialogs);

/** The user stays: the held-back quit is dropped. */
function stay() {
  store.setQuitDuringBackup(false);
  void Promise.resolve(api.cancelQuit()).catch(() => {});
}

/** Asks what to do when Questkeep should quit (or update) while a backup is being written. */
export function QuitDuringBackupDialog() {
  const [waiting, setWaiting] = createSignal(false);

  async function quit(wait: boolean) {
    setWaiting(wait);
    try {
      await api.finishQuit(wait);
    } catch (error) {
      setWaiting(false);
      store.pushToast("error", errorMessage(error));
    }
  }

  return (
    <Modal title={tr("backupTitle")} onClose={stay}>
      <Show when={waiting()} fallback={<p>{tr("backupWriting")}</p>}>
        <p role="status">
          {tr("quitsWhenDone", {
            label: store.backgroundWorkLabel() ?? tr("finishingBackup"),
          })}
        </p>
      </Show>
      <div class="form__actions">
        <button type="button" class="btn" onClick={stay}>
          {tr("keepOpen")}
        </button>
        <button type="button" class="btn" onClick={() => void quit(false)}>
          {tr("quitNow")}
        </button>
        <button
          type="button"
          class="btn btn--primary"
          data-autofocus
          disabled={waiting()}
          onClick={() => void quit(true)}
        >
          {tr("waitAndQuit")}
        </button>
      </div>
    </Modal>
  );
}
