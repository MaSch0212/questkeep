import { createResource, createSignal, For, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { formatPlaytime } from "../lib/format";
import { store } from "../lib/store";
import { titleWhenCut } from "../lib/truncation";
import { GameIcon } from "./GameIcon";
import { Modal } from "./Modal";
import { SourceIcons, storeNames } from "./SourceIcon";

const groupKey = (group: readonly Game[]) => group.map((g) => g.id).join();

/**
 * Games that look like the same game in several stores. Each group can be combined into one
 * game (the first keeps its name, cover and details) or marked as different games.
 */
export function DuplicatesDialog(props: { onClose: () => void }) {
  const [groups, { mutate }] = createResource(() => api.findDuplicates());
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const shown = () => groups.latest ?? [];

  async function settle(group: readonly Game[], work: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    try {
      await work();
      mutate((list) => list?.filter((g) => groupKey(g) !== groupKey(group)));
      store.scheduleRefresh();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }

  const combine = (group: readonly Game[]) => {
    const [first, ...rest] = group;
    if (!first) return Promise.resolve();
    return settle(group, () =>
      api.combineGames(
        first.id,
        rest.map((g) => g.id),
      ),
    );
  };

  /** One group after another: each combine changes the library the next one reads. */
  function combineAll() {
    return shown().reduce<Promise<void>>(
      (previous, group) => previous.then(() => combine(group)),
      Promise.resolve(),
    );
  }

  return (
    <Modal title="Games in several stores" wide onClose={props.onClose}>
      <div class="duplicates">
        <Show
          when={!groups.loading || shown().length > 0}
          fallback={<p class="muted">Looking for games in several stores…</p>}
        >
          <Show
            when={shown().length > 0}
            fallback={<p class="muted">No games found that are in more than one store.</p>}
          >
            <p class="muted">
              These look like the same game in different stores. Combined, they show as one game
              with every store, and Play starts the installed copy. You can separate them again on
              the game's page.
            </p>
            <ul class="duplicates__list">
              <For each={shown()}>
                {(group) => (
                  <li class="duplicates__group">
                    <ul class="duplicates__games" aria-label={group[0]?.name}>
                      <For each={group}>
                        {(game) => (
                          <li class="duplicates__game">
                            <span class="list-row__thumb">
                              <GameIcon game={game} />
                            </span>
                            <span
                              class="duplicates__name"
                              onPointerEnter={(event) => titleWhenCut(event.currentTarget)}
                            >
                              {game.name}
                            </span>
                            <SourceIcons game={game} />
                            <span class="duplicates__store">{storeNames(game)}</span>
                            <span class="duplicates__playtime">
                              {formatPlaytime(game.playtimeSeconds)}
                            </span>
                          </li>
                        )}
                      </For>
                    </ul>
                    <div class="duplicates__actions">
                      <button
                        type="button"
                        class="btn btn--small"
                        disabled={busy()}
                        aria-label={`${group[0]?.name ?? ""} are different games`}
                        onClick={() =>
                          void settle(group, () => api.dismissDuplicates(group.map((g) => g.id)))
                        }
                      >
                        Not the same
                      </button>
                      <button
                        type="button"
                        class="btn btn--small btn--primary"
                        disabled={busy()}
                        aria-label={`Combine ${group[0]?.name ?? ""}`}
                        onClick={() => void combine(group)}
                      >
                        Combine
                      </button>
                    </div>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </Show>
        <Show when={error()}>
          <p class="error-text" role="alert">
            {error()}
          </p>
        </Show>
        <div class="form__actions">
          <button type="button" class="btn" onClick={props.onClose}>
            Close
          </button>
          <Show when={shown().length > 1}>
            <button
              type="button"
              class="btn btn--primary"
              disabled={busy()}
              onClick={() => void combineAll()}
            >
              Combine all ({shown().length})
            </button>
          </Show>
        </div>
      </div>
    </Modal>
  );
}
