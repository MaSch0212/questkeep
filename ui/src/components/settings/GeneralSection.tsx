import { For, Show } from "solid-js";
import type { StartupPage } from "../../bindings/StartupPage";
import type { WindowOnGameStart } from "../../bindings/WindowOnGameStart";
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

/** What happens to Questkeep's window when a game starts. */
const ON_GAME_START: readonly [
  WindowOnGameStart,
  "onStartNothing" | "onStartMinimize" | "onStartHideToTray",
][] = [
  ["nothing", "onStartNothing"],
  ["minimize", "onStartMinimize"],
  ["hideToTray", "onStartHideToTray"],
];

/** The most a launcher is kept open after a game, and the longest session that never closes one. */
const MAX_CLOSE_DELAY_SECONDS = 3600;
const MAX_CLOSE_MIN_MINUTES = 600;

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

/** Saves a whole number in 0..max from a number input; anything else puts the old value back. */
function saveWhole(
  input: HTMLInputElement,
  previous: number,
  max: number,
  save: (value: number) => void,
) {
  const value = Math.round(Number(input.value));
  if (input.value.trim() !== "" && Number.isFinite(value) && value >= 0 && value <= max) {
    save(value);
  } else input.value = String(previous);
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
            <label class="check">
              <input
                type="checkbox"
                checked={current().showStatistics}
                onChange={(e) =>
                  void props.model.update({ showStatistics: e.currentTarget.checked })
                }
              />
              <span>{tr("showStatistics")}</span>
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
          <fieldset class="fieldset settings-group">
            <legend class="fieldset__legend">{tr("gameStartsGroup")}</legend>
            <div class="settings-row">
              <div
                class="segmented"
                role="radiogroup"
                aria-label={tr("gameStartsGroup")}
                aria-describedby="game-starts-hint"
              >
                <For each={ON_GAME_START}>
                  {([value, label]) => (
                    <button
                      type="button"
                      role="radio"
                      class="segmented__option"
                      aria-checked={current().windowOnGameStart === value}
                      onClick={() => void props.model.update({ windowOnGameStart: value })}
                    >
                      {tr(label)}
                    </button>
                  )}
                </For>
              </div>
            </div>
            <label
              class="check"
              classList={{ "check--disabled": current().windowOnGameStart === "nothing" }}
            >
              <input
                type="checkbox"
                checked={current().restoreWindowAfterGame}
                disabled={current().windowOnGameStart === "nothing"}
                aria-describedby="restore-window-hint"
                onChange={(e) =>
                  void props.model.toggle("restoreWindowAfterGame", e.currentTarget.checked)
                }
              />
              <span>{tr("restoreAfterGame")}</span>
            </label>
            <p class="field__hint" id="restore-window-hint">
              {tr("restoreHint")}
            </p>
            <p class="field__hint" id="game-starts-hint">
              {tr("gameStartsHint")}
            </p>
          </fieldset>
          <fieldset class="fieldset settings-group">
            <legend class="fieldset__legend">{tr("afterPlayingGroup")}</legend>
            <div class="settings-row">
              <label for="close-delay">{tr("closeDelay")}</label>
              <input
                id="close-delay"
                class="input settings-row__number"
                type="number"
                min="0"
                max={MAX_CLOSE_DELAY_SECONDS}
                aria-describedby="close-delay-hint"
                value={current().launcherCloseDelaySeconds}
                onChange={(e) =>
                  saveWhole(
                    e.currentTarget,
                    current().launcherCloseDelaySeconds,
                    MAX_CLOSE_DELAY_SECONDS,
                    (launcherCloseDelaySeconds) =>
                      void props.model.update({ launcherCloseDelaySeconds }),
                  )
                }
              />
              <span class="field__hint" id="close-delay-hint">
                {tr("closeDelayUnit")} · {tr("closeDelayHint")}
              </span>
            </div>
            <div class="settings-row">
              <label for="close-min">{tr("closeMinMinutes")}</label>
              <input
                id="close-min"
                class="input settings-row__number"
                type="number"
                min="0"
                max={MAX_CLOSE_MIN_MINUTES}
                aria-describedby="close-min-unit"
                value={current().launcherCloseMinMinutes}
                onChange={(e) =>
                  saveWhole(
                    e.currentTarget,
                    current().launcherCloseMinMinutes,
                    MAX_CLOSE_MIN_MINUTES,
                    (launcherCloseMinMinutes) =>
                      void props.model.update({ launcherCloseMinMinutes }),
                  )
                }
              />
              <span class="field__hint" id="close-min-unit">
                {tr("closeMinUnit")}
              </span>
            </div>
            <p class="field__hint">{tr("afterPlayingHint")}</p>
          </fieldset>
          <fieldset class="fieldset settings-group">
            <legend class="fieldset__legend">{tr("notificationsGroup")}</legend>
            <label class="check">
              <input
                type="checkbox"
                checked={current().systemNotifications}
                aria-describedby="system-notifications-hint"
                onChange={(e) =>
                  void props.model.toggle("systemNotifications", e.currentTarget.checked)
                }
              />
              <span>{tr("systemNotifications")}</span>
            </label>
            <p class="field__hint" id="system-notifications-hint">
              {tr("systemNotificationsHint")}
            </p>
          </fieldset>
        </>
      )}
    </Show>
  );
}
