import { openPath } from "@tauri-apps/plugin-opener";
import { Show } from "solid-js";
import { errorMessage } from "../../lib/errors";
import { store } from "../../lib/store";

async function openFolder(path: string) {
  try {
    await openPath(path);
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

export function AboutSection() {
  return (
    <Show when={store.appInfo()} fallback={<p class="muted">Loading…</p>}>
      {(info) => (
        <dl class="facts">
          <dt>Version</dt>
          <dd>{info().version}</dd>
          <dt>Data directory</dt>
          <dd class="facts__path">
            <span class="facts__path-text selectable">{info().dataDir}</span>
            <button type="button" class="btn" onClick={() => void openFolder(info().dataDir)}>
              Open folder
            </button>
          </dd>
          <dt>Logs directory</dt>
          <dd class="facts__path">
            <span class="facts__path-text selectable">{info().logsDir}</span>
            <button type="button" class="btn" onClick={() => void openFolder(info().logsDir)}>
              Open folder
            </button>
          </dd>
        </dl>
      )}
    </Show>
  );
}
