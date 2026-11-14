import Check from "lucide-solid/icons/check";
import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import type { Category } from "../bindings/Category";
import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import { translator } from "../i18n";
import { settingsLibrary } from "../i18n/settingsLibrary";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { defaultQuery } from "../lib/query";
import { store } from "../lib/store";
import { activeFilterCount } from "../lib/views";
import { FilterFields } from "./FilterPanel";
import { GameIcon } from "./GameIcon";
import { Modal } from "./Modal";

const tr = translator(settingsLibrary);

const PREVIEW_DELAY_MS = 150;
/** Rows the preview lists; the count above it says how many match in all. */
const PREVIEW_ROWS = 200;

/**
 * Edits the filter of an auto category: the same filters as a view, with a live list of the games
 * that match. Saving gives them the category; games added or changed later get it as they
 * match. Games keep it when they stop matching, and one taken off by hand is not given it again.
 */
export function CategoryFilterDialog(props: { category: Category; onClose: () => void }) {
  const [query, setQuery] = createSignal<GameQuery>(props.category.autoFilter ?? defaultQuery());
  const [matches, setMatches] = createSignal<Game[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));

  createEffect(
    on(query, (current) => {
      clearTimeout(timer);
      const mine = ++seq;
      timer = setTimeout(() => {
        void Promise.resolve(api.listGames({ ...current, search: null }))
          .then((games) => {
            if (mine === seq) setMatches(games ?? []);
          })
          .catch((failure: unknown) => {
            if (mine === seq) setError(errorMessage(failure));
          });
      }, PREVIEW_DELAY_MS);
    }),
  );

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
      onClose={props.onClose}
      footer={footer}
    >
      <div class="auto-filter">
        <div class="auto-filter__filters">
          <p class="muted">{tr("autoIntro")}</p>
          <FilterFields
            query={query()}
            onChange={(patch) => setQuery((current) => ({ ...current, ...patch }))}
          />
        </div>
        <section class="auto-filter__preview" aria-label={tr("autoPreview")}>
          <h3 class="auto-filter__heading" role="status">
            <Show when={matches()} fallback={tr("autoLoading")}>
              {(list) =>
                empty() ? tr("autoPickFilter") : tr("autoMatches", { count: list().length })
              }
            </Show>
          </h3>
          <Show when={error()}>
            <p class="error-text" role="alert">
              {error()}
            </p>
          </Show>
          <Show when={!empty()}>
            <ul class="auto-filter__list">
              <For each={(matches() ?? []).slice(0, PREVIEW_ROWS)}>
                {(game) => (
                  <li class="auto-filter__game">
                    <GameIcon game={game} />
                    <span class="auto-filter__name">{game.name}</span>
                    <Show when={has(game)}>
                      <span class="auto-filter__has" data-tooltip={tr("autoHasIt")}>
                        <Check class="icon" aria-hidden="true" />
                        <span class="visually-hidden">{tr("autoHasIt")}</span>
                      </span>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
            <Show when={(matches()?.length ?? 0) > PREVIEW_ROWS}>
              <p class="muted">
                {tr("autoMore", { count: (matches()?.length ?? 0) - PREVIEW_ROWS })}
              </p>
            </Show>
          </Show>
        </section>
      </div>
    </Modal>
  );
}
