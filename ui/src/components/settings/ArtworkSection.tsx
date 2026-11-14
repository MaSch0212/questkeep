import Eye from "lucide-solid/icons/eye";
import EyeOff from "lucide-solid/icons/eye-off";
import { createSignal, Show } from "solid-js";
import { translator } from "../../i18n";
import { settingsGeneral } from "../../i18n/settingsGeneral";
import { settingsKeys } from "../../i18n/settingsKeys";
import { openLink } from "./openLink";
import type { SettingsModel } from "./useSettings";

const tr = translator(settingsKeys);
const general = translator(settingsGeneral);

const GRIDDB_KEY_URL = "https://www.steamgriddb.com/profile/preferences/api";

export function ArtworkSection(props: { model: SettingsModel }) {
  const [showKey, setShowKey] = createSignal(false);

  return (
    <Show when={props.model.settings()} fallback={<p class="muted">{tr("loading")}</p>}>
      <label class="check">
        <input
          type="checkbox"
          checked={props.model.settings()?.downloadCovers ?? false}
          onChange={(e) => void props.model.toggle("downloadCovers", e.currentTarget.checked)}
        />
        <span>{general("downloadCovers")}</span>
      </label>
      <form
        class="form"
        onSubmit={(event) => {
          event.preventDefault();
          void props.model.saveGridDb();
        }}
      >
        <p class="muted">
          {tr("artworkIntro")}{" "}
          <button type="button" class="link-btn" onClick={() => void openLink(GRIDDB_KEY_URL)}>
            {GRIDDB_KEY_URL}
          </button>
          .
        </p>
        <div class="field">
          <label class="field__label" for="griddb-api-key">
            {tr("griddbKey")}
          </label>
          <div class="field__row">
            <input
              id="griddb-api-key"
              class="input"
              type={showKey() ? "text" : "password"}
              autocomplete="off"
              spellcheck={false}
              value={props.model.gridDbKey()}
              onInput={(e) => props.model.setGridDbKey(e.currentTarget.value)}
            />
            <button
              type="button"
              class="icon-btn icon-btn--bordered"
              aria-label={showKey() ? tr("hideGriddbKey") : tr("showGriddbKey")}
              aria-pressed={showKey()}
              data-tooltip={showKey() ? tr("hideKey") : tr("showKey")}
              onClick={() => setShowKey((v) => !v)}
            >
              <Show when={showKey()} fallback={<Eye class="icon" aria-hidden="true" />}>
                <EyeOff class="icon" aria-hidden="true" />
              </Show>
            </button>
          </div>
        </div>
        <p class="field__hint">{tr("griddbHint")}</p>
        <div class="form__actions">
          <button type="submit" class="btn btn--primary" disabled={props.model.savingGridDb()}>
            {props.model.savingGridDb() ? tr("saving") : tr("saveArtwork")}
          </button>
        </div>
      </form>
    </Show>
  );
}
