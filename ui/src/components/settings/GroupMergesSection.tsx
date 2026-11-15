import { For, Show } from "solid-js";
import { translator } from "../../i18n";
import { libraryGroupMerge } from "../../i18n/libraryGroupMerge";
import { store } from "../../lib/store";
import { GROUP_FIELDS, groupLabel } from "../../lib/viewOptions";
import { GroupMergeList } from "../GroupMergeList";

const tr = translator(libraryGroupMerge);

/** The grouping fields that have combined groups. */
const fields = () => GROUP_FIELDS.filter((f) => (store.groupMerges()[f]?.length ?? 0) > 0);

/** Every combined group, by grouping field, with Edit and Split up. */
export function GroupMergesSection() {
  return (
    <div class="form">
      <p class="muted">{tr("settingsIntro")}</p>
      <Show when={fields().length > 0} fallback={<p class="muted">{tr("noMerges")}</p>}>
        <For each={fields()}>
          {(field) => (
            <section aria-label={groupLabel(field)}>
              <h3 class="merge-fields__title">{groupLabel(field)}</h3>
              <GroupMergeList field={field} />
            </section>
          )}
        </For>
      </Show>
    </div>
  );
}
