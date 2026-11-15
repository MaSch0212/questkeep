import { createSignal, For, Show } from "solid-js";
import type { GroupBy } from "../bindings/GroupBy";
import type { GroupMerge } from "../bindings/GroupMerge";
import type { GroupMergeMember } from "../bindings/GroupMergeMember";
import { translator } from "../i18n";
import { libraryGroupMerge } from "../i18n/libraryGroupMerge";
import { shellCommon } from "../i18n/shellCommon";
import { NO_CATEGORY } from "../lib/views";
import { store } from "../lib/store";
import { groupLabel, inSentence } from "../lib/viewOptions";
import { SourceIcon } from "./SourceIcon";
import { Modal } from "./Modal";

const tr = translator(libraryGroupMerge);
const trCommon = translator(shellCommon);

/** A choice in the dialog: a group of the open view, or a member the view does not have now. */
interface Choice {
  key: string;
  label: string;
  source: string | null;
  count: number | null;
  /** The other combined group it belongs to. */
  takenBy: string | null;
}

/**
 * Combines groups of a grouping field under one name, or edits or splits up a
 * combined group. The combination is saved per grouping field, so every view grouped the same
 * way shows it (the dialog says so).
 */
export function GroupMergeDialog(props: {
  /** The grouping field whose groups are combined. */
  field: GroupBy;
  /** Groups ticked at the start. */
  keys: string[];
  /** The combined group being edited; null for a new one. */
  mergeId: string | null;
  onClose: () => void;
}) {
  const field = props.field;
  const merges = store.groupMerges()[field] ?? [];
  const editing = merges.find((m) => m.id === props.mergeId) ?? null;
  const takenBy = new Map<string, string>();
  for (const merge of merges) {
    if (merge.id === editing?.id) continue;
    for (const member of merge.members) takenBy.set(member.key, merge.name);
  }
  const plain = store.plainGroupsBy(field).filter((g) => g.key !== NO_CATEGORY && g.key !== "");
  const choices: Choice[] = [
    ...plain.map((g) => ({
      key: g.key,
      label: g.label,
      source: g.source,
      count: g.games.length,
      takenBy: takenBy.get(g.key) ?? null,
    })),
    // Members of this merge that the open view has no games for.
    ...(editing?.members ?? [])
      .filter((m) => !plain.some((g) => g.key === m.key))
      .map((m) => ({ key: m.key, label: m.label, source: null, count: null, takenBy: null })),
  ];
  const [name, setName] = createSignal(editing?.name ?? "");
  const [picked, setPicked] = createSignal<ReadonlySet<string>>(
    new Set(editing ? editing.members.map((m) => m.key) : props.keys),
  );
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const valid = () => name().trim() !== "" && picked().size >= 2;

  function toggle(key: string, on: boolean) {
    setPicked((current) => {
      const next = new Set(current);
      if (on) next.add(key);
      else next.delete(key);
      return next;
    });
  }

  async function save(next: GroupMerge[]) {
    setBusy(true);
    setError(null);
    const failure = await store.saveGroupMerges(field, next);
    setBusy(false);
    if (failure) setError(failure);
    else props.onClose();
  }

  function submit(event: Event) {
    event.preventDefault();
    if (!valid() || busy()) return;
    const members: GroupMergeMember[] = choices
      .filter((c) => picked().has(c.key))
      .map((c) => ({ key: c.key, label: c.label }));
    const merge: GroupMerge = {
      id: editing?.id ?? crypto.randomUUID(),
      name: name().trim(),
      members,
    };
    // Groups taken from another combined group leave it; an edited merge keeps its place.
    const rest = merges
      .map((m) =>
        m.id === merge.id
          ? merge
          : { ...m, members: m.members.filter((x) => !picked().has(x.key)) },
      )
      .filter((m) => m.members.length > 0);
    void save(editing ? rest : [...rest, merge]);
  }

  return (
    <Modal title={editing ? tr("editTitle") : tr("title")} onClose={props.onClose}>
      <form class="form group-merge" onSubmit={submit}>
        <p class="field__hint">{tr("notice", { field: inSentence(groupLabel(field)) })}</p>
        <label class="field">
          <span class="field__label">{tr("name")}</span>
          <input
            class="input"
            data-autofocus
            value={name()}
            placeholder={tr("namePlaceholder")}
            onInput={(e) => setName(e.currentTarget.value)}
          />
        </label>
        <fieldset class="field group-merge__groups">
          <legend class="field__label">{tr("groups")}</legend>
          <ul class="group-merge__list">
            <For each={choices}>
              {(choice) => (
                <li>
                  <label class="check" classList={{ "check--disabled": busy() }}>
                    <input
                      type="checkbox"
                      checked={picked().has(choice.key)}
                      disabled={busy()}
                      onChange={(e) => toggle(choice.key, e.currentTarget.checked)}
                    />
                    <Show when={choice.source}>{(source) => <SourceIcon source={source()} />}</Show>
                    <span class="group-merge__label">{choice.label}</span>
                    <span class="group-merge__meta">
                      {choice.count === null
                        ? tr("notInView")
                        : choice.takenBy
                          ? `${choice.count} · ${tr("inOther", { name: choice.takenBy })}`
                          : choice.count}
                    </span>
                  </label>
                </li>
              )}
            </For>
          </ul>
        </fieldset>
        <Show when={!valid()}>
          <p class="field__hint">{tr("pickTwo")}</p>
        </Show>
        <Show when={error()}>
          {(message) => (
            <p class="error-text" role="alert">
              {message()}
            </p>
          )}
        </Show>
        <div class="form__actions">
          <Show when={editing}>
            {(merge) => (
              <button
                type="button"
                class="btn btn--danger group-merge__split"
                disabled={busy()}
                onClick={() => void save(merges.filter((m) => m.id !== merge().id))}
              >
                {tr("split")}
              </button>
            )}
          </Show>
          <button type="button" class="btn" onClick={props.onClose}>
            {trCommon("cancel")}
          </button>
          <button type="submit" class="btn btn--primary" disabled={!valid() || busy()}>
            {tr("save")}
          </button>
        </div>
      </form>
    </Modal>
  );
}
