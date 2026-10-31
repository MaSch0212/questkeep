import { createResource, createSignal, For, Match, Show, Switch } from "solid-js";
import type { CombinePlan } from "../bindings/CombinePlan";
import type { Game } from "../bindings/Game";
import * as api from "../lib/api";
import {
  combineRows,
  suggestedPlan,
  type CombineRow,
  type CombineSource,
} from "../lib/combinePlan";
import { errorMessage } from "../lib/errors";
import { formatDate, formatPlaytime } from "../lib/format";
import { store } from "../lib/store";
import { Cover } from "./Cover";
import { Modal } from "./Modal";
import { sourceName } from "./SourceIcon";

const SECTIONS = [
  { id: "game", label: "Game" },
  { id: "you", label: "You" },
] as const;

/** Loads what the combine screen shows about each game. */
async function loadSources(ids: readonly string[]): Promise<CombineSource[]> {
  return Promise.all(
    ids.map(async (id) => {
      const game = store.games().find((g) => g.id === id) ?? (await api.getGame(id));
      const [edits, profiles] = await Promise.all([
        api.gameEditInfo(id).catch(() => null),
        api.launchProfiles(id).catch(() => null),
      ]);
      return { game, edits, profiles };
    }),
  );
}

const latest = (games: Game[], pick: (g: Game) => string | null, newest: boolean) =>
  games
    .map(pick)
    .filter((d): d is string => d !== null)
    .toSorted()
    .at(newest ? -1 : 0) ?? null;

/**
 * The combine screen: one column per game, one row per value that differs. Single values are
 * a choice (radio), values that can be combined are checkboxes; playtime and history add up.
 */
export function CombineDialog(props: { ids: string[]; onClose: () => void }) {
  const [sources] = createResource(() => props.ids, loadSources);
  const [plan, setPlan] = createSignal<CombinePlan | null>(null);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const suggested = () => {
    const list = sources();
    return list ? suggestedPlan(list, store.completion().statuses) : null;
  };
  const current = () => plan() ?? suggested();
  const games = () => sources()?.map((s) => s.game) ?? [];
  const rows = () =>
    combineRows(sources() ?? [], {
      status: (id) => store.completion().statuses.find((s) => s.id === id)?.name ?? null,
      category: (id) => store.categories().find((c) => c.id === id)?.name ?? id,
      source: sourceName,
    });

  function change(patch: Partial<CombinePlan>) {
    const now = current();
    if (now) setPlan({ ...now, ...patch });
  }

  function isChosen(row: CombineRow, id: string, value: string | null): boolean {
    const now = current();
    if (!now) return false;
    if (row.kind === "favorite") return (value === "Yes") === now.favorite;
    if (row.kind === "single") return now[row.key] === id;
    return now[row.key].includes(id);
  }

  function choose(row: CombineRow, id: string, value: string | null) {
    const now = current();
    if (!now) return;
    if (row.kind === "favorite") change({ favorite: value === "Yes" });
    else if (row.kind === "single") change({ [row.key]: id });
    else {
      const list = now[row.key];
      change({ [row.key]: list.includes(id) ? list.filter((x) => x !== id) : [...list, id] });
    }
  }

  async function combine() {
    const now = current();
    if (!now || saving()) return;
    setSaving(true);
    setError(null);
    try {
      const game = await api.combineWithPlan(now);
      store.upsertGame(game);
      store.selectGame(game.id);
      store.scheduleRefresh(0);
      void store.loadCategories();
      store.pushToast("info", `Combined ${props.ids.length} games into “${game.name}”`);
      props.onClose();
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  return (
    <Modal title={`Combine ${props.ids.length} games into one`} wide onClose={props.onClose}>
      <div class="combine">
        <p class="muted">
          Each game&apos;s store copies are kept. Choose which values the combined game keeps;
          values with a check box can be kept together.
        </p>
        <Show when={sources()} fallback={<p class="muted">Loading…</p>}>
          <div class="combine__scroll">
            <table class="combine__table">
              <thead>
                <tr>
                  <th scope="col">
                    <span class="visually-hidden">Value</span>
                  </th>
                  <For each={games()}>
                    {(game) => (
                      <th scope="col" class="combine__game">
                        <span class="combine__thumb">
                          <Cover game={game} />
                        </span>
                        <span class="combine__name">{game.name}</span>
                        <small class="muted">
                          {sourceName(game.source)} · {formatPlaytime(game.playtimeSeconds)}
                        </small>
                      </th>
                    )}
                  </For>
                </tr>
              </thead>
              <tbody>
                <For each={SECTIONS}>
                  {(section) => (
                    <Show when={rows().some((r) => r.section === section.id)}>
                      <tr class="combine__section">
                        <th scope="rowgroup" colspan={games().length + 1}>
                          {section.label}
                        </th>
                      </tr>
                      <For each={rows().filter((r) => r.section === section.id)}>
                        {(row) => (
                          <tr>
                            <th scope="row" class="combine__field">
                              {row.label}
                            </th>
                            <For each={games()}>
                              {(game, index) => {
                                const value = () => row.values[index()] ?? null;
                                return (
                                  <td>
                                    <Show
                                      when={value() !== null}
                                      fallback={<span class="combine__empty">none</span>}
                                    >
                                      <label
                                        class="combine__option"
                                        classList={{
                                          "combine__option--on": isChosen(row, game.id, value()),
                                        }}
                                      >
                                        <input
                                          type={row.kind === "multi" ? "checkbox" : "radio"}
                                          name={row.label}
                                          checked={isChosen(row, game.id, value())}
                                          onChange={() => choose(row, game.id, value())}
                                        />
                                        <span class="combine__value" title={value() ?? ""}>
                                          {value()}
                                        </span>
                                      </label>
                                    </Show>
                                  </td>
                                );
                              }}
                            </For>
                          </tr>
                        )}
                      </For>
                    </Show>
                  )}
                </For>
                <tr class="combine__section">
                  <th scope="rowgroup" colspan={games().length + 1}>
                    Added up
                  </th>
                </tr>
                <tr>
                  <th scope="row" class="combine__field">
                    Playtime
                  </th>
                  <td colspan={games().length}>
                    <strong>
                      {formatPlaytime(games().reduce((sum, g) => sum + g.playtimeSeconds, 0))}
                    </strong>{" "}
                    <span class="muted">all play history is kept</span>
                  </td>
                </tr>
                <tr>
                  <th scope="row" class="combine__field">
                    Last played / added
                  </th>
                  <td colspan={games().length}>
                    <strong>
                      {formatDate(latest(games(), (g) => g.lastPlayed, true))} /{" "}
                      {formatDate(latest(games(), (g) => g.dateAdded, false))}
                    </strong>{" "}
                    <span class="muted">latest played, earliest added</span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </Show>
        <Switch>
          <Match when={error()}>{(message) => <p class="error-text">{message()}</p>}</Match>
        </Switch>
        <div class="combine__foot">
          <button
            type="button"
            class="btn btn--ghost"
            disabled={plan() === null}
            onClick={() => setPlan(null)}
          >
            Reset to suggested
          </button>
          <span class="combine__buttons">
            <button type="button" class="btn" onClick={props.onClose}>
              Cancel
            </button>
            <button
              type="button"
              class="btn btn--primary"
              disabled={!current() || saving()}
              onClick={() => void combine()}
            >
              {saving() ? "Combining…" : "Combine"}
            </button>
          </span>
        </div>
      </div>
    </Modal>
  );
}
