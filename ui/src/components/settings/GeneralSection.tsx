import { For, Show } from "solid-js";
import type { StartupPage } from "../../bindings/StartupPage";
import { LOCALE_NAMES, LOCALES, resolveLocale, translator } from "../../i18n";
import { settingsGeneral } from "../../i18n/settingsGeneral";
import { allViews, shownName } from "../../lib/views";
import { store } from "../../lib/store";
import { ChipInput } from "../ChipInput";
import type { SettingsModel } from "./useSettings";

/** Common leading words offered while typing (English, German, French, Spanish). */
const SUGGESTED_PREFIXES = ["The", "A", "An", "Der", "Die", "Das", "Le", "La", "Les", "El", "Los"];

const tr = translator(settingsGeneral);

/** The automatic sync's choices, in minutes (0: off). */
const AUTO_SYNC_MINUTES = [0, 15, 60] as const;

/** Check boxes saved as they are; the label is the message with the setting's name. */
type CheckSetting =
  | "scanOnStartup"
  | "watchInstallFolders"
  | "detectExternalLaunches"
  | "closeToTray"
  | "minimizeToTray"
  | "startWithSystem";

/** The language the system would give (shown next to "System language"). */
const systemLocale = () => resolveLocale(null, navigator.languages ?? [navigator.language]);

/** The dropdown value of a startup page: "start", "last" or "view:<id>". */
function startupValue(page: StartupPage): string {
  return page.kind === "view" ? `view:${page.id}` : page.kind;
}

function startupPage(value: string): StartupPage {
  if (value.startsWith("view:")) return { kind: "view", id: value.slice("view:".length) };
  return value === "last" ? { kind: "last" } : { kind: "start" };
}

function CheckOption(props: { model: SettingsModel; option: CheckSetting; checked: boolean }) {
  return (
    <label class="check">
      <input
        type="checkbox"
        checked={props.checked}
        onChange={(e) => void props.model.toggle(props.option, e.currentTarget.checked)}
      />
      <span>{tr(props.option)}</span>
    </label>
  );
}

export function GeneralSection(props: { model: SettingsModel }) {
  return (
    <Show when={props.model.settings()} fallback={<p class="muted">{tr("loading")}</p>}>
      {(current) => (
        <>
          <div class="settings-row">
            <label for="language">{tr("language")}</label>
            <select
              id="language"
              class="input settings-row__input"
              value={current().language ?? ""}
              onChange={(e) => void props.model.update({ language: e.currentTarget.value || null })}
            >
              <option value="">
                {tr("systemLanguage", { name: LOCALE_NAMES[systemLocale()] })}
              </option>
              <For each={LOCALES}>
                {(locale) => <option value={locale}>{LOCALE_NAMES[locale]}</option>}
              </For>
            </select>
          </div>
          <fieldset class="fieldset settings-group">
            <legend class="fieldset__legend">{tr("syncGroup")}</legend>
            <CheckOption
              model={props.model}
              option="scanOnStartup"
              checked={current().scanOnStartup}
            />
            <div class="settings-row">
              <label for="auto-sync">{tr("autoSync")}</label>
              <select
                id="auto-sync"
                class="input settings-row__input"
                aria-describedby="auto-sync-hint"
                value={String(current().autoSyncMinutes)}
                onChange={(e) =>
                  void props.model.update({ autoSyncMinutes: Number(e.currentTarget.value) })
                }
              >
                <For each={AUTO_SYNC_MINUTES}>
                  {(minutes) => (
                    <option value={String(minutes)}>
                      {minutes === 0
                        ? tr("autoSyncOff")
                        : minutes === 60
                          ? tr("autoSyncHourly")
                          : tr("autoSyncMinutes", { minutes })}
                    </option>
                  )}
                </For>
              </select>
            </div>
            <p class="field__hint" id="auto-sync-hint">
              {tr("autoSyncHint")}
            </p>
            <CheckOption
              model={props.model}
              option="watchInstallFolders"
              checked={current().watchInstallFolders}
            />
          </fieldset>
          <fieldset class="fieldset settings-group">
            <legend class="fieldset__legend">{tr("playtimeGroup")}</legend>
            <CheckOption
              model={props.model}
              option="detectExternalLaunches"
              checked={current().detectExternalLaunches}
            />
          </fieldset>
          <fieldset class="fieldset settings-group">
            <legend class="fieldset__legend">{tr("startPage")}</legend>
            <label class="check">
              <input
                type="checkbox"
                checked={current().showStartPage}
                onChange={(e) =>
                  void props.model.update({ showStartPage: e.currentTarget.checked })
                }
              />
              <span>{tr("showStartPage")}</span>
            </label>
            <div class="settings-row">
              <label for="startup-page">{tr("whenOpens")}</label>
              <select
                id="startup-page"
                class="input settings-row__input"
                value={startupValue(current().startupPage)}
                onChange={(e) =>
                  void props.model.update({ startupPage: startupPage(e.currentTarget.value) })
                }
              >
                <option value="start" disabled={!current().showStartPage}>
                  {tr("startPage")}
                </option>
                <option value="last">{tr("lastPage")}</option>
                <optgroup label={tr("views")}>
                  <For each={allViews(store.sections())}>
                    {(view) => <option value={`view:${view.id}`}>{shownName(view)}</option>}
                  </For>
                </optgroup>
              </select>
            </div>
            <Show when={!current().showStartPage && current().startupPage.kind === "start"}>
              <p class="field__hint">{tr("startPageOff")}</p>
            </Show>
          </fieldset>
          <fieldset class="fieldset settings-group">
            <legend class="fieldset__legend">{tr("sorting")}</legend>
            <label class="check">
              <input
                type="checkbox"
                checked={current().sortPrefixes.enabled}
                onChange={(e) =>
                  void props.model.update({
                    sortPrefixes: { ...current().sortPrefixes, enabled: e.currentTarget.checked },
                  })
                }
              />
              <span>{tr("ignorePrefixes")}</span>
            </label>
            <div
              class="settings-row"
              classList={{ "check--disabled": !current().sortPrefixes.enabled }}
            >
              <label for="sort-prefixes">{tr("wordsToIgnore")}</label>
              <ChipInput
                id="sort-prefixes"
                label={tr("wordsToIgnore")}
                values={current().sortPrefixes.words}
                suggestions={SUGGESTED_PREFIXES}
                describedBy="sort-prefixes-hint"
                onChange={(words) =>
                  void props.model.update({ sortPrefixes: { ...current().sortPrefixes, words } })
                }
              />
            </div>
            <p class="field__hint" id="sort-prefixes-hint">
              {tr("prefixesHint")}
            </p>
          </fieldset>
          <fieldset class="fieldset settings-group">
            <legend class="fieldset__legend">{tr("windowGroup")}</legend>
            <CheckOption
              model={props.model}
              option="startWithSystem"
              checked={current().startWithSystem}
            />
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
              <span>{tr("startMinimized")}</span>
            </label>
            <CheckOption model={props.model} option="closeToTray" checked={current().closeToTray} />
            <CheckOption
              model={props.model}
              option="minimizeToTray"
              checked={current().minimizeToTray}
            />
          </fieldset>
        </>
      )}
    </Show>
  );
}
