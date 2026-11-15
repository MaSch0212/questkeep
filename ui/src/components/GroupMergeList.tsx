import Pencil from "lucide-solid/icons/pencil";
import Split from "lucide-solid/icons/split";
import { For, Show } from "solid-js";
import type { GroupBy } from "../bindings/GroupBy";
import { translator } from "../i18n";
import { libraryGroupMerge } from "../i18n/libraryGroupMerge";
import { store } from "../lib/store";

const tr = translator(libraryGroupMerge);

/**
 * The combined groups of one grouping field, each with Edit (opens the Combine groups dialog)
 * and Split up. Used by the Combined groups dialog and the settings overview.
 */
export function GroupMergeList(props: { field: GroupBy }) {
  const merges = () => store.groupMerges()[props.field] ?? [];

  function splitUp(id: string) {
    void store
      .saveGroupMerges(
        props.field,
        merges().filter((m) => m.id !== id),
      )
      .then((failure) => failure && store.pushToast("error", failure));
  }

  return (
    <Show when={merges().length > 0} fallback={<p class="muted">{tr("noMerges")}</p>}>
      <ul class="merge-list">
        <For each={merges()}>
          {(merge) => (
            <li class="merge-list__row">
              <span class="merge-list__text">
                <b class="merge-list__name">{merge.name}</b>
                <span class="merge-list__members">
                  {merge.members.map((m) => m.label).join(", ")}
                </span>
              </span>
              <span class="merge-list__buttons">
                <button
                  type="button"
                  class="btn"
                  aria-label={tr("editNamed", { name: merge.name })}
                  onClick={() =>
                    store.setMergeDialog({ field: props.field, keys: [], mergeId: merge.id })
                  }
                >
                  <Pencil class="icon" aria-hidden="true" />
                  {tr("edit")}
                </button>
                <button
                  type="button"
                  class="btn btn--ghost"
                  aria-label={tr("splitNamed", { name: merge.name })}
                  onClick={() => splitUp(merge.id)}
                >
                  <Split class="icon" aria-hidden="true" />
                  {tr("split")}
                </button>
              </span>
            </li>
          )}
        </For>
      </ul>
    </Show>
  );
}
