import { openUrl } from "@tauri-apps/plugin-opener";
import Eye from "lucide-solid/icons/eye";
import EyeOff from "lucide-solid/icons/eye-off";
import { createSignal, Show } from "solid-js";
import { errorMessage } from "../../lib/errors";
import { store } from "../../lib/store";
import type { SettingsModel } from "./useSettings";

const STEAM_KEY_URL = "https://steamcommunity.com/dev/apikey";

async function openLink(url: string) {
  try {
    await openUrl(url);
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

export function SteamSection(props: { model: SettingsModel }) {
  const [showKey, setShowKey] = createSignal(false);

  return (
    <Show when={props.model.settings()} fallback={<p class="muted">Loading…</p>}>
      <form
        class="form"
        onSubmit={(event) => {
          event.preventDefault();
          void props.model.saveSteam();
        }}
      >
        <p class="muted">
          The Steam Web API key lets Questkeep list games you own but have not installed. Create one
          at{" "}
          <button type="button" class="link-btn" onClick={() => void openLink(STEAM_KEY_URL)}>
            {STEAM_KEY_URL}
          </button>
          .
        </p>
        <div class="field">
          <label class="field__label" for="steam-api-key">
            Steam Web API key
          </label>
          <div class="field__row">
            <input
              id="steam-api-key"
              class="input"
              type={showKey() ? "text" : "password"}
              autocomplete="off"
              spellcheck={false}
              value={props.model.apiKey()}
              onInput={(e) => props.model.setApiKey(e.currentTarget.value)}
            />
            <button
              type="button"
              class="icon-btn icon-btn--bordered"
              aria-label={showKey() ? "Hide API key" : "Show API key"}
              aria-pressed={showKey()}
              title={showKey() ? "Hide API key" : "Show API key"}
              onClick={() => setShowKey((v) => !v)}
            >
              <Show when={showKey()} fallback={<Eye class="icon" aria-hidden="true" />}>
                <EyeOff class="icon" aria-hidden="true" />
              </Show>
            </button>
          </div>
        </div>
        <div class="field">
          <label class="field__label" for="steam-id">
            SteamID64 (optional)
          </label>
          <input
            id="steam-id"
            class="input"
            type="text"
            inputmode="numeric"
            autocomplete="off"
            aria-describedby="steam-id-hint"
            value={props.model.steamId()}
            onInput={(e) => props.model.setSteamId(e.currentTarget.value)}
          />
          <span id="steam-id-hint" class="field__hint">
            Leave empty to use the account last signed in to Steam.
          </span>
        </div>
        <p class="field__hint">
          The key is stored in your system's credential store, not in the Questkeep data folder.
        </p>
        <div class="form__actions">
          <button type="submit" class="btn btn--primary" disabled={props.model.savingSteam()}>
            {props.model.savingSteam() ? "Saving…" : "Save Steam account"}
          </button>
        </div>
      </form>
    </Show>
  );
}
