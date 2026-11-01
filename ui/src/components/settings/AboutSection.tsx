import { Show } from "solid-js";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { translator } from "../../i18n";
import { settingsPage } from "../../i18n/settingsPage";
import { store } from "../../lib/store";

const tr = translator(settingsPage);

async function openFolder(kind: "data" | "logs") {
  try {
    await api.openAppFolder(kind);
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

export function AboutSection() {
  return (
    <Show when={store.appInfo()} fallback={<p class="muted">{tr("loading")}</p>}>
      {(info) => (
        <dl class="facts">
          <dt>{tr("version")}</dt>
          <dd>{info().version}</dd>
          <dt>{tr("dataDir")}</dt>
          <dd class="facts__path">
            <span class="facts__path-text selectable">{info().dataDir}</span>
            <button type="button" class="btn" onClick={() => void openFolder("data")}>
              {tr("openFolder")}
            </button>
          </dd>
          <dt>{tr("logsDir")}</dt>
          <dd class="facts__path">
            <span class="facts__path-text selectable">{info().logsDir}</span>
            <button type="button" class="btn" onClick={() => void openFolder("logs")}>
              {tr("openFolder")}
            </button>
          </dd>
        </dl>
      )}
    </Show>
  );
}
