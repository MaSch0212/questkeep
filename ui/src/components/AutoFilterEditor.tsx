import Check from "lucide-solid/icons/check";
import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import { translator } from "../i18n";
import { settingsLibrary } from "../i18n/settingsLibrary";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { activeFilterCount } from "../lib/views";
import { FilterFields } from "./FilterPanel";
import { GameIcon } from "./GameIcon";

const tr = translator(settingsLibrary);

const PREVIEW_DELAY_MS = 150;
/** Rows the preview lists; the count above it says how many match in all. */
const PREVIEW_ROWS = 200;

/**
 * The filters of a query (the same as a view's) next to a live list of the games that match.
 * Shared by the auto category's filter and the filter of a script. The caller owns the query and
 * its dialog; `marked` puts a check mark on listed games (those that already have the category).
 */
export function AutoFilterEditor(props: {
  query: GameQuery;
  onChange: (query: GameQuery) => void;
  intro: string;
  marked?: (game: Game) => boolean;
  markedLabel?: string;
}) {
  const [matches, setMatches] = createSignal<Game[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));

  createEffect(
    on(
      () => props.query,
      (current) => {
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
      },
    ),
  );

  const empty = () => activeFilterCount(props.query) === 0;

  return (
    <div class="auto-filter">
      <div class="auto-filter__filters">
        <p class="muted">{props.intro}</p>
        <FilterFields
          query={props.query}
          onChange={(patch) => props.onChange({ ...props.query, ...patch })}
        />
      </div>
      <section class="auto-filter__preview" aria-label={tr("autoPreview")}>
        <h3 class="auto-filter__heading" role="status">
          {empty()
            ? tr("autoPickFilter")
            : matches() === null
              ? tr("autoLoading")
              : tr("autoMatches", { count: matches()?.length ?? 0 })}
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
                  <Show when={props.marked?.(game)}>
                    <span class="auto-filter__has" data-tooltip={props.markedLabel}>
                      <Check class="icon" aria-hidden="true" />
                      <span class="visually-hidden">{props.markedLabel}</span>
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
  );
}
