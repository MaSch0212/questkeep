import { createSignal, Show } from "solid-js";
import type { Category } from "../bindings/Category";
import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import { translator } from "../i18n";
import { settingsLibrary } from "../i18n/settingsLibrary";
import { defaultQuery } from "../lib/query";
import { store } from "../lib/store";
import { activeFilterCount } from "../lib/views";
import { AutoFilterEditor } from "./AutoFilterEditor";
import { Modal } from "./Modal";

const tr = translator(settingsLibrary);

/**
 * Edits the filter of an auto category: the same filters as a view, with a live list of the games
 * that match. Saving gives them the category; games added or changed later get it as they
 * match. Games keep it when they stop matching, and one taken off by hand is not given it again.
 */
export function CategoryFilterDialog(props: { category: Category; onClose: () => void }) {
  const [query, setQuery] = createSignal<GameQuery>(props.category.autoFilter ?? defaultQuery());
  const [saving, setSaving] = createSignal(false);

  const empty = () => activeFilterCount(query()) === 0;
  const has = (game: Game) => game.categories.includes(props.category.id);

  async function save(filter: GameQuery | null) {
    setSaving(true);
    const done = await store.setCategoryFilter(props.category.id, filter);
    setSaving(false);
    if (done) props.onClose();
  }

  const footer = (
    <div class="form__actions">
      <Show when={props.category.autoFilter}>
        <button
          type="button"
          class="btn btn--ghost auto-filter__remove"
          disabled={saving()}
          onClick={() => void save(null)}
        >
          {tr("autoRemove")}
        </button>
      </Show>
      <button type="button" class="btn" onClick={() => props.onClose()}>
        {tr("cancel")}
      </button>
      <button
        type="button"
        class="btn btn--primary"
        disabled={saving() || empty()}
        data-tooltip={empty() ? tr("autoPickFilter") : undefined}
        onClick={() => void save(query())}
      >
        {tr("autoSave")}
      </button>
    </div>
  );

  return (
    <Modal
      title={tr("autoTitle", { name: props.category.name })}
      wide
      tall
      onClose={props.onClose}
      footer={footer}
    >
      <AutoFilterEditor
        query={query()}
        onChange={setQuery}
        intro={tr("autoIntro")}
        marked={has}
        markedLabel={tr("autoHasIt")}
      />
    </Modal>
  );
}
