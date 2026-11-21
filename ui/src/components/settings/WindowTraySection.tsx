import { For, Show } from "solid-js";
import type { WindowOnGameStart } from "../../bindings/WindowOnGameStart";
import { translator } from "../../i18n";
import { settingsGeneral } from "../../i18n/settingsGeneral";
import { CheckOption, saveWhole } from "./SettingsControls";
import { TraySettings } from "./TraySettings";
import type { SettingsModel } from "./useSettings";

const tr = translator(settingsGeneral);

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

/** How Questkeep's window starts, closes and hides, the tray, and what happens around a game. */
export function WindowTraySection(props: { model: SettingsModel }) {
  return (
    <Show when={props.model.settings()} fallback={<p class="muted">{tr("loading")}</p>}>
      {(current) => (
        <>
          <fieldset class="fieldset settings-group">
            <legend class="fieldset__legend">{tr("windowGroup")}</legend>
            <CheckOption
              model={props.model}
              option="startWithSystem"
              checked={current().startWithSystem}
            />
            <label
              class="check check--switch check--nested"
              classList={{ "check--disabled": !current().startWithSystem }}
            >
              <input
                type="checkbox"
                role="switch"
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
            <legend class="fieldset__legend">{tr("trayGroup")}</legend>
            <TraySettings model={props.model} />
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
              class="check check--switch"
              classList={{ "check--disabled": current().windowOnGameStart === "nothing" }}
            >
              <input
                type="checkbox"
                role="switch"
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
              <div class="settings-row__text">
                <label for="close-delay">{tr("closeDelay")}</label>
                <span class="field__hint" id="close-delay-hint">
                  {tr("closeDelayHint")}
                </span>
              </div>
              <span class="input-unit">
                <input
                  id="close-delay"
                  class="input"
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
                <span class="input-unit__unit" aria-hidden="true">
                  {tr("closeDelayUnit")}
                </span>
              </span>
            </div>
            <div class="settings-row">
              <label for="close-min" class="settings-row__text">
                {tr("closeMinMinutes")}
              </label>
              <span class="input-unit">
                <input
                  id="close-min"
                  class="input"
                  type="number"
                  min="0"
                  max={MAX_CLOSE_MIN_MINUTES}
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
                <span class="input-unit__unit" aria-hidden="true">
                  {tr("closeMinUnit")}
                </span>
              </span>
            </div>
            <p class="field__hint">{tr("afterPlayingHint")}</p>
          </fieldset>
        </>
      )}
    </Show>
  );
}
