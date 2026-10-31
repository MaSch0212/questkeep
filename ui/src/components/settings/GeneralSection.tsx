import { For, Show } from "solid-js";
import type { StartupPage } from "../../bindings/StartupPage";
import { allViews } from "../../lib/views";
import { store } from "../../lib/store";
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

/** The dropdown value of a startup page: "start", "last" or "view:<id>". */
function startupValue(page: StartupPage): string {
  return page.kind === "view" ? `view:${page.id}` : page.kind;
}

function startupPage(value: string): StartupPage {
  if (value.startsWith("view:")) return { kind: "view", id: value.slice("view:".length) };
  return value === "last" ? { kind: "last" } : { kind: "start" };
}

function CheckOption(props: {
  model: SettingsModel;
  option: {
    key: (typeof OPTIONS)[number]["key"] | (typeof TRAY_OPTIONS)[number]["key"];
    label: string;
  };
  checked: boolean;
}) {
  return (
    <label class="check">
      <input
        type="checkbox"
        checked={props.checked}
        onChange={(e) => void props.model.toggle(props.option.key, e.currentTarget.checked)}
      />
      <span>{props.option.label}</span>
    </label>
  );
}

export function GeneralSection(props: { model: SettingsModel }) {
  return (
    <Show when={props.model.settings()} fallback={<p class="muted">Loading…</p>}>
      {(current) => (
        <>
          <For each={OPTIONS}>
            {(option) => (
              <CheckOption model={props.model} option={option} checked={current()[option.key]} />
            )}
          </For>
          <fieldset class="fieldset settings-group">
            <legend class="fieldset__legend">Start page</legend>
            <label class="check">
              <input
                type="checkbox"
                checked={current().showStartPage}
                onChange={(e) =>
                  void props.model.update({ showStartPage: e.currentTarget.checked })
                }
              />
              <span>Show the start page</span>
            </label>
            <div class="settings-row">
              <label for="startup-page">When Questkeep opens, show</label>
              <select
                id="startup-page"
                class="input settings-row__input"
                value={startupValue(current().startupPage)}
                onChange={(e) =>
                  void props.model.update({ startupPage: startupPage(e.currentTarget.value) })
                }
              >
                <option value="start" disabled={!current().showStartPage}>
                  Start page
                </option>
                <option value="last">The page I had open last</option>
                <optgroup label="Views">
                  <For each={allViews(store.sections())}>
                    {(view) => <option value={`view:${view.id}`}>{view.name}</option>}
                  </For>
                </optgroup>
              </select>
            </div>
            <Show when={!current().showStartPage && current().startupPage.kind === "start"}>
              <p class="field__hint">
                The start page is turned off, so Questkeep opens the first view instead.
              </p>
            </Show>
          </fieldset>
          <For each={TRAY_OPTIONS}>
            {(option) => (
              <CheckOption model={props.model} option={option} checked={current()[option.key]} />
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
