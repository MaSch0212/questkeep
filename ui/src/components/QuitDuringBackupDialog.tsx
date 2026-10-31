import { createSignal, Show } from "solid-js";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { store } from "../lib/store";
import { Modal } from "./Modal";

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
    <Modal title="A backup is running" onClose={stay}>
      <Show
        when={waiting()}
        fallback={
          <p>
            Questkeep is writing a backup. Quitting now cancels it and deletes the unfinished
            archive.
          </p>
        }
      >
        <p role="status">
          {store.backgroundWorkLabel() ?? "Finishing the backup…"} Questkeep quits when it is done.
        </p>
      </Show>
      <div class="form__actions">
        <button type="button" class="btn" onClick={stay}>
          Keep Questkeep open
        </button>
        <button type="button" class="btn" onClick={() => void quit(false)}>
          Quit now
        </button>
        <button
          type="button"
          class="btn btn--primary"
          data-autofocus
          disabled={waiting()}
          onClick={() => void quit(true)}
        >
          Wait and quit
        </button>
      </div>
    </Modal>
  );
}
