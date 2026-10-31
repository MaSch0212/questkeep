import Combine from "lucide-solid/icons/combine";
import Download from "lucide-solid/icons/download";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import Trash from "lucide-solid/icons/trash";
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { changeAll, installAll, refreshAll, removeAll } from "../lib/bulkActions";
import { formatPlaytime, formatSize } from "../lib/format";
import { readCssPx } from "../lib/layoutMode";
import { categoryCoverage, common, coverFan, storeCounts } from "../lib/selection";
import { store } from "../lib/store";
import { ConfirmDialog } from "./ConfirmDialog";
import { Cover } from "./Cover";
import { PanelHeader, type PanelPin } from "./PanelHeader";
import { sourceName } from "./SourceIcon";

/** Fallbacks for `--qk-stack-cover-width` and `--qk-stack-min-step` (theme.css). */
const COVER = 120;
const MIN_STEP = 22;

/** "Mixed" for differing values, else the value's label. */
const mixedLabel = <T,>(value: T | "mixed" | undefined, label: (v: T) => string) =>
  value === "mixed" ? "Mixed" : value === undefined ? "" : label(value);

/**
 * The details panel (or page) while several games are selected: a fanned stack of their covers,
 * counts and totals, "Change all" for shared fields and actions on all of them.
 */
export function MultiSelectPanel(props: {
  games: Game[];
  variant?: "panel" | "page";
  pin?: PanelPin;
  split?: boolean;
  onClose: () => void;
}) {
  const [width, setWidth] = createSignal(0);
  const [sizes, setSizes] = createSignal({ cover: COVER, step: MIN_STEP });
  const [removing, setRemoving] = createSignal(false);
  let stack: HTMLDivElement | undefined;

  onMount(() => {
    if (!stack) return;
    setSizes({
      cover: readCssPx(stack, "--qk-stack-cover-width", COVER),
      step: readCssPx(stack, "--qk-stack-min-step", MIN_STEP),
    });
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (rect) setWidth(rect.width);
    });
    observer.observe(stack);
    onCleanup(() => observer.disconnect());
  });

  const count = () => props.games.length;
  const fan = () => coverFan(count(), width() || sizes().cover * 3, sizes().cover, sizes().step);
  const installed = () => props.games.filter((g) => g.installed).length;
  const playtime = () => props.games.reduce((sum, g) => sum + g.playtimeSeconds, 0);
  const size = () => props.games.reduce((sum, g) => sum + (g.installSize ?? 0), 0);
  const favorite = () => common(props.games, (g) => g.favorite);
  const hidden = () => common(props.games, (g) => g.hidden);
  const status = () => common(props.games, (g) => g.completionStatus);
  const ids = () => props.games.map((g) => g.id);

  function onStatus(value: string) {
    void store.setCompletionStatus(ids(), value === "" ? null : value);
  }

  function toggleCategory(id: string) {
    const all = categoryCoverage(props.games, id) === "all";
    void changeAll(props.games, {
      addCategories: all ? [] : [id],
      removeCategories: all ? [id] : [],
    });
  }

  return (
    <aside
      class="details multi"
      classList={{
        "details--split": props.split === true,
        "details--page": props.variant === "page",
      }}
      aria-label={`${count()} games selected`}
    >
      <Show when={props.variant !== "page"}>
        <PanelHeader pin={props.pin ?? null} onClose={props.onClose} />
      </Show>
      <div class="details__scroll">
        <div class="multi__stack" ref={(el) => (stack = el)}>
          <For each={props.games.slice(0, fan().shown)}>
            {(game, index) => (
              <div
                class="multi__cover"
                style={{
                  transform: `translateX(${index() * fan().step}px) rotate(calc(${index() - (fan().shown - 1) / 2} * var(--qk-stack-tilt)))`,
                  "z-index": fan().shown - index(),
                }}
              >
                <Cover game={game} />
              </div>
            )}
          </For>
          <Show when={count() > fan().shown}>
            <span class="multi__more">+{count() - fan().shown}</span>
          </Show>
        </div>
        <div class="multi__head">
          <h2 class="details__title">{count()} games selected</h2>
          <p class="muted">{storeCounts(props.games, sourceName)}</p>
          <button type="button" class="link-btn" onClick={() => store.clearSelection()}>
            Clear selection
          </button>
        </div>
        <dl class="multi__totals">
          <div>
            <dt>Installed</dt>
            <dd>
              {installed()} of {count()}
            </dd>
          </div>
          <div>
            <dt>Playtime</dt>
            <dd>{playtime() > 0 ? formatPlaytime(playtime()) : "None"}</dd>
          </div>
          <Show when={size() > 0}>
            <div>
              <dt>Install size</dt>
              <dd>{formatSize(size())}</dd>
            </div>
          </Show>
        </dl>

        <section class="details-card" aria-labelledby="multi-change">
          <h3 id="multi-change" class="details-card__title">
            Change all
          </h3>
          <label class="multi__row">
            <span>Completion status</span>
            <select class="input" onChange={(e) => onStatus(e.currentTarget.value)}>
              <Show when={status() === "mixed"}>
                <option value="mixed" disabled selected>
                  Mixed
                </option>
              </Show>
              <option value="" selected={status() === null}>
                None
              </option>
              <For each={store.completion().statuses}>
                {(s) => (
                  <option value={s.id} selected={status() === s.id}>
                    {s.name}
                  </option>
                )}
              </For>
            </select>
          </label>
          <div class="multi__row">
            <span>Favorite</span>
            <button
              type="button"
              class="btn"
              aria-pressed={favorite() === true}
              onClick={() =>
                void changeAll(props.games, {
                  favorite: favorite() !== true,
                  addCategories: [],
                  removeCategories: [],
                })
              }
            >
              {mixedLabel(favorite(), (v) => (v ? "Yes" : "No"))}
            </button>
          </div>
          <div class="multi__row">
            <span>Hidden</span>
            <button
              type="button"
              class="btn"
              aria-pressed={hidden() === true}
              onClick={() =>
                void changeAll(props.games, {
                  hidden: hidden() !== true,
                  addCategories: [],
                  removeCategories: [],
                })
              }
            >
              {mixedLabel(hidden(), (v) => (v ? "Yes" : "No"))}
            </button>
          </div>
          <Show when={store.categories().length > 0}>
            <div class="multi__categories" role="group" aria-label="Categories">
              <span class="muted">Categories</span>
              <div class="chips">
                <For each={store.categories()}>
                  {(category) => {
                    const coverage = () => categoryCoverage(props.games, category.id);
                    return (
                      <button
                        type="button"
                        class="chip multi__chip"
                        classList={{
                          "multi__chip--all": coverage() === "all",
                          "multi__chip--some": coverage() === "some",
                        }}
                        aria-pressed={
                          coverage() === "all" ? true : coverage() === "some" ? "mixed" : false
                        }
                        title={
                          coverage() === "all"
                            ? `Take ${category.name} off all`
                            : `Add ${category.name} to all`
                        }
                        onClick={() => toggleCategory(category.id)}
                      >
                        {category.name}
                      </button>
                    );
                  }}
                </For>
              </div>
            </div>
          </Show>
        </section>

        <div class="multi__actions">
          <Show when={installed() < count()}>
            <button type="button" class="btn" onClick={() => void installAll(props.games)}>
              <Download class="icon" aria-hidden="true" />
              Install all
            </button>
          </Show>
          <button type="button" class="btn" onClick={() => void refreshAll(props.games)}>
            <RefreshCw class="icon" aria-hidden="true" />
            Refresh metadata
          </button>
          <button type="button" class="btn" onClick={() => store.openCombine(ids())}>
            <Combine class="icon" aria-hidden="true" />
            Combine into one game…
          </button>
          <button type="button" class="btn btn--danger" onClick={() => setRemoving(true)}>
            <Trash class="icon" aria-hidden="true" />
            Remove from library…
          </button>
        </div>
      </div>
      <Show when={removing()}>
        <ConfirmDialog
          title="Remove games"
          message={<>Remove {count()} games from your library?</>}
          confirmLabel="Remove"
          onConfirm={() => void removeAll(props.games)}
          onClose={() => setRemoving(false)}
        />
      </Show>
    </aside>
  );
}
