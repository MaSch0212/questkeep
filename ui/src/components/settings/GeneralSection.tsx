import { For, Show } from "solid-js";
import type { SettingsModel } from "./useSettings";

const OPTIONS = [
  { key: "scanOnStartup", label: "Scan stores when the app starts" },
  { key: "downloadCovers", label: "Download missing covers" },
  { key: "fetchMetadata", label: "Download store metadata" },
] as const;

export function GeneralSection(props: { model: SettingsModel }) {
  return (
    <Show when={props.model.settings()} fallback={<p class="muted">Loading…</p>}>
      {(current) => (
        <For each={OPTIONS}>
          {(option) => (
            <label class="check">
              <input
                type="checkbox"
                checked={current()[option.key]}
                onChange={(e) => void props.model.toggle(option.key, e.currentTarget.checked)}
              />
              <span>{option.label}</span>
            </label>
          )}
        </For>
      )}
    </Show>
  );
}
