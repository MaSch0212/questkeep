import Eye from "lucide-solid/icons/eye";
import EyeOff from "lucide-solid/icons/eye-off";
import { createSignal, Show } from "solid-js";
import { openLink } from "./openLink";
import type { SettingsModel } from "./useSettings";

const GRIDDB_KEY_URL = "https://www.steamgriddb.com/profile/preferences/api";

export function ArtworkSection(props: { model: SettingsModel }) {
  const [showKey, setShowKey] = createSignal(false);

  return (
    <Show when={props.model.settings()} fallback={<p class="muted">Loading…</p>}>
      <form
        class="form"
        onSubmit={(event) => {
          event.preventDefault();
          void props.model.saveGridDb();
        }}
      >
        <p class="muted">
          SteamGridDB is used as a fallback for covers and backgrounds when a store has none. Create
          an API key at{" "}
          <button type="button" class="link-btn" onClick={() => void openLink(GRIDDB_KEY_URL)}>
            {GRIDDB_KEY_URL}
          </button>
          .
        </p>
        <div class="field">
          <label class="field__label" for="griddb-api-key">
            SteamGridDB API key
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
              aria-label={showKey() ? "Hide SteamGridDB key" : "Show SteamGridDB key"}
              aria-pressed={showKey()}
              title={showKey() ? "Hide key" : "Show key"}
              onClick={() => setShowKey((v) => !v)}
            >
              <Show when={showKey()} fallback={<Eye class="icon" aria-hidden="true" />}>
                <EyeOff class="icon" aria-hidden="true" />
              </Show>
            </button>
          </div>
        </div>
        <p class="field__hint">
          The key is stored unencrypted in the Questkeep data folder. Only use a key you are
          comfortable keeping there. Leave it empty to turn SteamGridDB off.
        </p>
        <div class="form__actions">
          <button type="submit" class="btn btn--primary" disabled={props.model.savingGridDb()}>
            {props.model.savingGridDb() ? "Saving…" : "Save artwork settings"}
          </button>
        </div>
      </form>
    </Show>
  );
}
