import Eye from "lucide-solid/icons/eye";
import EyeOff from "lucide-solid/icons/eye-off";
import { createSignal, Show } from "solid-js";
import { openLink } from "./openLink";
import type { SettingsModel } from "./useSettings";

const TWITCH_CONSOLE_URL = "https://dev.twitch.tv/console/apps";

export function MetadataSection(props: { model: SettingsModel }) {
  const [showSecret, setShowSecret] = createSignal(false);

  return (
    <Show when={props.model.settings()} fallback={<p class="muted">Loading…</p>}>
      <form
        class="form"
        onSubmit={(event) => {
          event.preventDefault();
          void props.model.saveIgdb();
        }}
      >
        <p class="muted">
          IGDB fills in the description, genres, developer, publisher and release date for games
          whose store has none, such as games you added by hand. It needs a free Twitch application:
          register one at{" "}
          <button type="button" class="link-btn" onClick={() => void openLink(TWITCH_CONSOLE_URL)}>
            {TWITCH_CONSOLE_URL}
          </button>{" "}
          and copy its Client ID and Client Secret here.
        </p>
        <div class="field">
          <label class="field__label" for="igdb-client-id">
            IGDB Client ID
          </label>
          <input
            id="igdb-client-id"
            class="input"
            type="text"
            autocomplete="off"
            spellcheck={false}
            value={props.model.igdbClientId()}
            onInput={(e) => props.model.setIgdbClientId(e.currentTarget.value)}
          />
        </div>
        <div class="field">
          <label class="field__label" for="igdb-client-secret">
            IGDB Client Secret
          </label>
          <div class="field__row">
            <input
              id="igdb-client-secret"
              class="input"
              type={showSecret() ? "text" : "password"}
              autocomplete="off"
              spellcheck={false}
              value={props.model.igdbClientSecret()}
              onInput={(e) => props.model.setIgdbClientSecret(e.currentTarget.value)}
            />
            <button
              type="button"
              class="icon-btn icon-btn--bordered"
              aria-label={showSecret() ? "Hide IGDB secret" : "Show IGDB secret"}
              aria-pressed={showSecret()}
              title={showSecret() ? "Hide secret" : "Show secret"}
              onClick={() => setShowSecret((v) => !v)}
            >
              <Show when={showSecret()} fallback={<Eye class="icon" aria-hidden="true" />}>
                <EyeOff class="icon" aria-hidden="true" />
              </Show>
            </button>
          </div>
        </div>
        <p class="field__hint">
          Both values are stored in your system's credential store, not in the Questkeep data
          folder. Leave them empty to turn IGDB off. Store data is never replaced.
        </p>
        <div class="form__actions">
          <button type="submit" class="btn btn--primary" disabled={props.model.savingIgdb()}>
            {props.model.savingIgdb() ? "Saving…" : "Save metadata settings"}
          </button>
        </div>
      </form>
    </Show>
  );
}
