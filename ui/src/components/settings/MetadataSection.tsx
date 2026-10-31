import { Show } from "solid-js";
import { IgdbFields, IgdbSteps } from "./KeyFields";
import { SourcePriority } from "./SourcePriority";
import type { SettingsModel } from "./useSettings";

export function MetadataSection(props: { model: SettingsModel }) {
  return (
    <Show when={props.model.settings()} fallback={<p class="muted">Loading…</p>}>
      <form
        class="form"
        onSubmit={(event) => {
          event.preventDefault();
          void props.model.saveIgdb();
        }}
      >
        <p class="muted">
          IGDB adds descriptions, genres, series, age ratings, game modes, themes, time to beat and
          links, also for games you added by hand. It needs a free Twitch application:
        </p>
        <IgdbSteps />
        <IgdbFields model={props.model} />
        <p class="field__hint">
          Both values are stored in your system's credential store, not in the Questkeep data
          folder. Leave them empty to turn IGDB off.
        </p>
        <div class="form__actions">
          <button type="submit" class="btn btn--primary" disabled={props.model.savingIgdb()}>
            {props.model.savingIgdb() ? "Saving…" : "Save metadata settings"}
          </button>
        </div>
      </form>
      <SourcePriority model={props.model} />
    </Show>
  );
}
