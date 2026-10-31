import { For, Show } from "solid-js";
import type { SettingsModel } from "./useSettings";

const OPTIONS = [
  { key: "scanOnStartup", label: "Scan stores when the app starts" },
  { key: "downloadCovers", label: "Download missing covers" },
  { key: "fetchMetadata", label: "Download store metadata" },
  {
    key: "detectExternalLaunches",
    label: "Record playtime of games started outside Questkeep",
  },
] as const;

const TRAY_OPTIONS = [
  { key: "closeToTray", label: "Close to the tray instead of quitting" },
  { key: "minimizeToTray", label: "Minimize to the tray" },
  { key: "startWithSystem", label: "Start Questkeep when I sign in to my computer" },
] as const;

export function GeneralSection(props: { model: SettingsModel }) {
  return (
    <Show when={props.model.settings()} fallback={<p class="muted">Loading…</p>}>
      {(current) => (
        <>
          <For each={[...OPTIONS, ...TRAY_OPTIONS]}>
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
          <label
            class="check check--nested"
            classList={{ "check--disabled": !current().startWithSystem }}
          >
            <input
              type="checkbox"
              checked={current().startMinimized}
              disabled={!current().startWithSystem}
              onChange={(e) => void props.model.toggle("startMinimized", e.currentTarget.checked)}
            />
            <span>Start minimized to the tray</span>
          </label>
        </>
      )}
    </Show>
  );
}
