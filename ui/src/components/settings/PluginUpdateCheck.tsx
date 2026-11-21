import { createSignal, Show } from "solid-js";
import { translator } from "../../i18n";
import { pluginUpdates } from "../../i18n/pluginUpdates";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { store } from "../../lib/store";
import type { SettingsModel } from "./useSettings";

const tr = translator(pluginUpdates);

/**
 * Settings → Plugins: the update check's switch, "Check now" and the Updates button that opens
 * the Plugin updates dialog (with the number of updates found).
 */
export function PluginUpdateCheck(props: { model: SettingsModel }) {
  const [checking, setChecking] = createSignal(false);
  /** What the last "Check now" found. */
  const [message, setMessage] = createSignal<string | null>(null);

  async function checkNow() {
    if (checking()) return;
    setChecking(true);
    setMessage(null);
    try {
      const check = await api.checkPluginUpdatesNow();
      await store.loadPluginUpdates().catch(() => undefined);
      setMessage(
        !check.requested
          ? tr("nothingToCheck")
          : check.updates.length === 0
            ? tr("upToDate")
            : tr("found", { count: check.updates.length }),
      );
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      setChecking(false);
    }
  }

  return (
    <div class="plugin-update-check" data-plugin-update-check>
      <Show when={props.model.settings()}>
        {(current) => (
          <label class="check check--switch">
            <input
              type="checkbox"
              role="switch"
              checked={current().checkPluginUpdates}
              aria-describedby="plugin-update-check-hint"
              onChange={(e) =>
                void props.model.toggle("checkPluginUpdates", e.currentTarget.checked)
              }
            />
            <span>{tr("checkSetting")}</span>
          </label>
        )}
      </Show>
      <p class="field__hint" id="plugin-update-check-hint">
        {tr("checkHint")}
      </p>
      <div class="plugin-install">
        <button
          type="button"
          class="btn"
          data-plugin-check-now
          disabled={checking()}
          aria-busy={checking()}
          onClick={() => void checkNow()}
        >
          {checking() ? tr("checking") : tr("checkNow")}
        </button>
        <Show when={store.pluginUpdates().length > 0}>
          <button
            type="button"
            class="btn btn--primary"
            data-plugin-updates-open
            aria-label={tr("updatesCount", { count: store.pluginUpdates().length })}
            onClick={() => store.openPluginUpdates()}
          >
            {tr("updatesButton")}
            <span class="account__badge plugin-card__update" aria-hidden="true">
              {store.pluginUpdates().length}
            </span>
          </button>
        </Show>
      </div>
      <Show when={message()}>
        {(text) => (
          <p class="muted" role="status">
            {text()}
          </p>
        )}
      </Show>
    </div>
  );
}
