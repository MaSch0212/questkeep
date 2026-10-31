import { For, Show } from "solid-js";
import type { StoreOption } from "../../bindings/StoreOption";
import { providerFlag } from "../../lib/providerSettings";
import type { SettingsModel } from "./useSettings";

/** A store's own switches (declared by the store), shown in its row of the Stores settings. */
export function StoreOptions(props: {
  providerId: string;
  options: readonly StoreOption[];
  model: SettingsModel;
}) {
  return (
    <Show when={props.model.settings()}>
      {(current) => (
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
      )}
    </Show>
  );
}
