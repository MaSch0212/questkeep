import Combine from "lucide-solid/icons/combine";
import { createSignal, For, Show } from "solid-js";
import type { GroupBy } from "../../bindings/GroupBy";
import { translator } from "../../i18n";
import { libraryGroupMerge } from "../../i18n/libraryGroupMerge";
import { store } from "../../lib/store";
import { GROUP_FIELDS, groupLabel } from "../../lib/viewOptions";
import { GroupMergeList } from "../GroupMergeList";

const tr = translator(libraryGroupMerge);

/** The grouping fields groups can be combined in. */
const COMBINABLE = GROUP_FIELDS.filter((f) => f !== "none");

/** The grouping fields that have combined groups. */
const fields = () => COMBINABLE.filter((f) => (store.groupMerges()[f]?.length ?? 0) > 0);

/**
 * Every combined group, by grouping field, with Edit and Split up, and a way to combine groups of
 * any grouping field (choosing from the whole library).
 */
export function GroupMergesSection() {
  const [field, setField] = createSignal<GroupBy>("genre");
  return (
    <div class="form">
      <p class="muted">{tr("settingsIntro")}</p>
      <div class="field__row merge-add">
        <select
          class="input merge-add__field"
          aria-label={tr("groupingField")}
          value={field()}
          onChange={(event) => setField(event.currentTarget.value as GroupBy)}
        >
          <For each={COMBINABLE}>{(id) => <option value={id}>{groupLabel(id)}</option>}</For>
        </select>
        <button
          type="button"
          class="btn"
          onClick={() =>
            store.setMergeDialog({ field: field(), keys: [], mergeId: null, wholeLibrary: true })
          }
        >
          <Combine class="icon" aria-hidden="true" />
          {tr("newMerge")}
        </button>
      </div>
      <Show when={fields().length > 0} fallback={<p class="muted">{tr("noMerges")}</p>}>
        <For each={fields()}>
          {(id) => (
            <section aria-label={groupLabel(id)}>
              <h3 class="merge-fields__title">{groupLabel(id)}</h3>
              <GroupMergeList field={id} wholeLibrary />
            </section>
          )}
        </For>
      </Show>
    </div>
  );
}
