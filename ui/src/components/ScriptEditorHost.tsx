import { createResource, Show } from "solid-js";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { store } from "../lib/store";
import { ScriptDialog } from "./ScriptDialog";

/**
 * The script dialog opened from outside Settings (the bell's "Edit script"): loads the script,
 * then edits it. A script that is gone (or fails to load) closes it with an error toast.
 */
export function ScriptEditorHost(props: { id: string }) {
  const [script] = createResource(
    () => props.id,
    async (id) => {
      try {
        return await api.getScript(id);
      } catch (failure) {
        store.pushToast("error", errorMessage(failure));
        store.closeScriptEditor();
        return null;
      }
    },
  );
  return (
    <Show when={script()}>
      {(loaded) => <ScriptDialog script={loaded()} onClose={() => store.closeScriptEditor()} />}
    </Show>
  );
}
