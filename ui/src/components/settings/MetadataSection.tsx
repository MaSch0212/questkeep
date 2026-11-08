import { Show } from "solid-js";
import { translator } from "../../i18n";
import { settingsGeneral } from "../../i18n/settingsGeneral";
import { settingsKeys } from "../../i18n/settingsKeys";
import { IgdbFields, IgdbSteps } from "./KeyFields";
import { SourcePriority } from "./SourcePriority";
import type { SettingsModel } from "./useSettings";

const tr = translator(settingsKeys);
const general = translator(settingsGeneral);

export function MetadataSection(props: { model: SettingsModel }) {
  return (
    <Show when={props.model.settings()} fallback={<p class="muted">{tr("loading")}</p>}>
      <label class="check">
        <input
          type="checkbox"
          checked={props.model.settings()?.fetchMetadata ?? false}
          onChange={(e) => void props.model.toggle("fetchMetadata", e.currentTarget.checked)}
        />
        <span>{general("fetchMetadata")}</span>
      </label>
      <form
        class="form"
        onSubmit={(event) => {
          event.preventDefault();
          void props.model.saveIgdb();
        }}
      >
        <p class="muted">{tr("igdbIntro")}</p>
        <IgdbSteps />
        <IgdbFields model={props.model} />
        <p class="field__hint">{tr("igdbHint")}</p>
        <div class="form__actions">
          <button type="submit" class="btn btn--primary" disabled={props.model.savingIgdb()}>
            {props.model.savingIgdb() ? tr("saving") : tr("saveMetadata")}
          </button>
        </div>
      </form>
      <SourcePriority model={props.model} />
    </Show>
  );
}
