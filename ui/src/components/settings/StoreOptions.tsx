import { Show } from "solid-js";
import {
  GOG_LAUNCH_THROUGH_GALAXY,
  UBISOFT_IMPORT_CATALOGUE,
  XBOX_HIDE_GAME_PASS,
  providerFlag,
} from "../../lib/providerSettings";
import type { SettingsModel } from "./useSettings";

interface StoreOption {
  key: string;
  label: string;
  hint: string;
}

/** Each store's own settings, shown in its row of the Stores settings. */
const OPTIONS: Record<string, StoreOption> = {
  gog: {
    key: GOG_LAUNCH_THROUGH_GALAXY,
    label: "Launch GOG games through GOG Galaxy",
    hint: "Galaxy adds cloud saves, its overlay and playtime tracking. Games Galaxy doesn't know (offline installers) and PCs without Galaxy still start the game directly.",
  },
  xbox: {
    key: XBOX_HIDE_GAME_PASS,
    label: "Don't import Game Pass games from my Xbox account",
    hint: "Games in PC Game Pass are only listed while installed. Xbox can't tell a bought game from a Game Pass one, so games you bought that are also in Game Pass are left out too. Games already in your library stay; hide or remove them there.",
  },
  ubisoft: {
    key: UBISOFT_IMPORT_CATALOGUE,
    label: "Import Ubisoft Connect games that are not installed",
    hint: "Ubisoft has no sign-in for other apps, so these come from the list of games Ubisoft Connect keeps on this PC (DLC and games sold through other stores are left out). It can include games you only looked at or played in a free weekend, and test versions. Turning this off again keeps the games in your library; hide or remove them there.",
  },
};

/** Whether the store has options of its own. */
export function hasStoreOptions(providerId: string): boolean {
  return providerId in OPTIONS;
}

export function StoreOptions(props: { providerId: string; model: SettingsModel }) {
  const option = () => OPTIONS[props.providerId];
  return (
    <Show when={option()}>
      {(opt) => (
        <Show when={props.model.settings()}>
          {(current) => (
            <div class="account__options">
              <label class="check">
                <input
                  type="checkbox"
                  checked={providerFlag(current(), props.providerId, opt().key)}
                  onChange={(e) =>
                    void props.model.setProviderOption(
                      props.providerId,
                      opt().key,
                      e.currentTarget.checked,
                    )
                  }
                />
                <span>{opt().label}</span>
              </label>
              <p class="field__hint">{opt().hint}</p>
            </div>
          )}
        </Show>
      )}
    </Show>
  );
}
