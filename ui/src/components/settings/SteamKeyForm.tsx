import Eye from "lucide-solid/icons/eye";
import EyeOff from "lucide-solid/icons/eye-off";
import { createSignal, Show } from "solid-js";
import { openLink } from "./openLink";
import type { SettingsModel } from "./useSettings";

const STEAM_KEY_URL = "https://steamcommunity.com/dev/apikey";

/** The Steam Web API key, the alternative to signing in to Steam (shown in the Steam row). */
export function SteamKeyForm(props: { model: SettingsModel }) {
  const [showKey, setShowKey] = createSignal(false);

  return (
    <form
      class="form"
      onSubmit={(event) => {
        event.preventDefault();
        void props.model.saveSteam();
      }}
    >
      <p class="field__hint">
        A key needs no sign-in, but only works when your profile&apos;s game details are public.
        Create one at{" "}
        <button type="button" class="link-btn" onClick={() => void openLink(STEAM_KEY_URL)}>
          {STEAM_KEY_URL}
        </button>
        . It is stored in your system&apos;s credential store.
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
          Leave empty to use the account last signed in to the Steam client.
        </span>
      </div>
      <div class="form__actions">
        <button
          type="submit"
          class="btn btn--primary"
          disabled={props.model.savingSteam() || props.model.apiKey().trim() === ""}
        >
          {props.model.savingSteam() ? "Saving…" : "Save Steam key"}
        </button>
      </div>
    </form>
  );
}
