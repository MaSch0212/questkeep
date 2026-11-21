import { translator } from "../../i18n";
import { settingsGeneral } from "../../i18n/settingsGeneral";
import type { SettingsModel } from "./useSettings";

const tr = translator(settingsGeneral);

/** Switches saved as they are; the label is the message named like the setting. */
export type CheckSetting =
  | "scanOnStartup"
  | "watchInstallFolders"
  | "detectExternalLaunches"
  | "closeToTray"
  | "minimizeToTray"
  | "startWithSystem";

/** A switch for a boolean setting that is saved when it is toggled. */
export function CheckOption(props: {
  model: SettingsModel;
  option: CheckSetting;
  checked: boolean;
}) {
  return (
    <label class="check check--switch">
      <input
        type="checkbox"
        role="switch"
        checked={props.checked}
        onChange={(e) => void props.model.toggle(props.option, e.currentTarget.checked)}
      />
      <span>{tr(props.option)}</span>
    </label>
  );
}

/** Saves a whole number in 0..max from a number input; anything else puts the old value back. */
export function saveWhole(
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
