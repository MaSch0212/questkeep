import Combine from "lucide-solid/icons/combine";
import type { GroupBy } from "../bindings/GroupBy";
import { translator } from "../i18n";
import { libraryGroupMerge } from "../i18n/libraryGroupMerge";
import { store } from "../lib/store";
import { groupLabel, inSentence } from "../lib/viewOptions";
import { GroupMergeList } from "./GroupMergeList";
import { Modal } from "./Modal";

const tr = translator(libraryGroupMerge);

/**
 * Combine groups… in the group menu: every combined group of the grouping field, to edit or
 * split up, and a button to combine more.
 */
export function GroupMergesDialog(props: { field: GroupBy; onClose: () => void }) {
  return (
    <Modal title={tr("managerTitle")} onClose={props.onClose}>
      <div class="form group-merges">
        <p class="field__hint">
          {tr("managerHint", { field: inSentence(groupLabel(props.field)) })}
        </p>
        <GroupMergeList field={props.field} />
        <div class="form__actions">
          <button
            type="button"
            class="btn group-merges__new"
            data-autofocus
            onClick={() => store.setMergeDialog({ field: props.field, keys: [], mergeId: null })}
          >
            <Combine class="icon" aria-hidden="true" />
            {tr("newMerge")}
          </button>
          <button type="button" class="btn btn--primary" onClick={() => props.onClose()}>
            {tr("close")}
          </button>
        </div>
      </div>
    </Modal>
  );
}
