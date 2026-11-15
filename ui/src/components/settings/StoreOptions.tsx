import { For, Show } from "solid-js";
import type { LauncherClose } from "../../bindings/LauncherClose";
import type { LauncherClosePolicy } from "../../bindings/LauncherClosePolicy";
import type { StoreOption } from "../../bindings/StoreOption";
import { translator } from "../../i18n";
import { settingsAccounts } from "../../i18n/settingsAccounts";
import {
  CLOSE_POLICIES,
  closeKind,
  launcherClosePolicy,
  withLauncherClose,
} from "../../lib/launcherClose";
import { providerFlag } from "../../lib/providerSettings";
import type { SettingsModel } from "./useSettings";

const tr = translator(settingsAccounts);

const POLICY_LABEL = {
  never: "closeNever",
  always: "closeAlways",
  ifStartedByGame: "closeIfStarted",
} as const satisfies Record<LauncherClosePolicy, string>;

const HINT = {
  command: "closeHintCommand",
  closeWindows: "closeHintWindows",
  forceQuit: "closeHintForce",
} as const;

/**
 * A store's own switches (declared by the store) and, for a store with a launcher Questkeep can
 * close, the "Close <store> after playing" choice; shown in its row of the Stores settings.
 */
export function StoreOptions(props: {
  providerId: string;
  storeName: string;
  options: readonly StoreOption[];
  launcherClose: LauncherClose | null;
  model: SettingsModel;
}) {
  return (
    <Show when={props.model.settings()}>
      {(current) => (
        <>
          <For each={props.options}>
            {(option) => (
              <div class="account__options">
                <label class="check">
                  <input
                    type="checkbox"
                    checked={providerFlag(current(), props.providerId, option.key)}
                    onChange={(e) =>
                      void props.model.setProviderOption(
                        props.providerId,
                        option.key,
                        e.currentTarget.checked,
                      )
                    }
                  />
                  <span>{option.label}</span>
                </label>
                <p class="field__hint">{option.hint}</p>
              </div>
            )}
          </For>
          <Show when={props.launcherClose}>
            {(close) => (
              <div class="account__options">
                <span class="field__label" id={`close-${props.providerId}`}>
                  {tr("closeAfterPlaying", { name: props.storeName })}
                </span>
                <div
                  class="segmented"
                  role="radiogroup"
                  aria-labelledby={`close-${props.providerId}`}
                  aria-describedby={`close-hint-${props.providerId}`}
                >
                  <For each={CLOSE_POLICIES}>
                    {(policy) => (
                      <button
                        type="button"
                        role="radio"
                        class="segmented__option"
                        aria-checked={launcherClosePolicy(current(), props.providerId) === policy}
                        onClick={() =>
                          void props.model.update({
                            launcherClose: withLauncherClose(
                              current().launcherClose,
                              props.providerId,
                              policy,
                            ),
                          })
                        }
                      >
                        {tr(POLICY_LABEL[policy])}
                      </button>
                    )}
                  </For>
                </div>
                <p class="field__hint" id={`close-hint-${props.providerId}`}>
                  {tr(HINT[closeKind(close())], {
                    name: props.storeName,
                    seconds: current().launcherCloseDelaySeconds,
                  })}
                </p>
              </div>
            )}
          </Show>
        </>
      )}
    </Show>
  );
}
