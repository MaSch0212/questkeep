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

/** Check boxes saved as they are; the label is the message with the setting's name. */
const OPTIONS = [
  "scanOnStartup",
  "downloadCovers",
  "fetchMetadata",
  "detectExternalLaunches",
] as const;

const TRAY_OPTIONS = ["closeToTray", "minimizeToTray", "startWithSystem"] as const;

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

function CheckOption(props: {
  model: SettingsModel;
  option: (typeof OPTIONS)[number] | (typeof TRAY_OPTIONS)[number];
  checked: boolean;
}) {
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
          <For each={OPTIONS}>
            {(option) => (
              <CheckOption model={props.model} option={option} checked={current()[option]} />
            )}
          </For>
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
          <For each={TRAY_OPTIONS}>
            {(option) => (
              <CheckOption model={props.model} option={option} checked={current()[option]} />
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
            <span>{tr("startMinimized")}</span>
          </label>
        </>
      )}
    </Show>
  );
}
