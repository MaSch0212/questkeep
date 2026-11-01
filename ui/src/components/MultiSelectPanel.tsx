import Combine from "lucide-solid/icons/combine";
import Download from "lucide-solid/icons/download";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import Trash from "lucide-solid/icons/trash";
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { translator } from "../i18n";
import { shellMulti } from "../i18n/shellMulti";
import { changeAll, installAll, refreshAll, removeAll } from "../lib/bulkActions";
import { formatPlaytime, formatSize } from "../lib/format";
import { readCssPx } from "../lib/layoutMode";
import { categoryCoverage, common, coverFan, storeCounts } from "../lib/selection";
import { store } from "../lib/store";
import { ConfirmDialog } from "./ConfirmDialog";
import { Cover } from "./Cover";
import { PanelHeader, type PanelPin } from "./PanelHeader";
import { sourceName } from "./SourceIcon";

const tr = translator(shellMulti);

/** Fallbacks for `--qk-stack-cover-width` and `--qk-stack-min-step` (theme.css). */
const COVER = 120;
const MIN_STEP = 22;

/** "Mixed" for differing values, else the value's label. */
const mixedLabel = <T,>(value: T | "mixed" | undefined, label: (v: T) => string) =>
  value === "mixed" ? tr("mixed") : value === undefined ? "" : label(value);

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
  const kind = () => common(props.games, (g) => g.kind);
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
      aria-label={tr("gamesSelected", { count: count() })}
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
            {/* At the corner of the last cover, above all of them. */}
            <span
              class="multi__more"
              style={{
                left: `${(fan().shown - 1) * fan().step + sizes().cover}px`,
                "z-index": fan().shown + 1,
              }}
            >
              +{count() - fan().shown}
            </span>
          </Show>
        </div>
        <div class="multi__head">
          <h2 class="details__title">{tr("gamesSelected", { count: count() })}</h2>
          <p class="muted">{storeCounts(props.games, sourceName)}</p>
          <button type="button" class="link-btn" onClick={() => store.clearSelection()}>
            {tr("clearSelection")}
          </button>
        </div>
        <dl class="multi__totals">
          <div>
            <dt>{tr("installed")}</dt>
            <dd>{tr("installedOf", { installed: installed(), count: count() })}</dd>
          </div>
          <div>
            <dt>{tr("playtime")}</dt>
            <dd>{playtime() > 0 ? formatPlaytime(playtime()) : tr("noPlaytime")}</dd>
          </div>
          <Show when={size() > 0}>
            <div>
              <dt>{tr("installSize")}</dt>
              <dd>{formatSize(size())}</dd>
            </div>
          </Show>
        </dl>

        <section class="details-card" aria-labelledby="multi-change">
          <h3 id="multi-change" class="details-card__title">
            {tr("changeAll")}
          </h3>
          <label class="multi__row">
            <span>{tr("completionStatus")}</span>
            <select class="input" onChange={(e) => onStatus(e.currentTarget.value)}>
              <Show when={status() === "mixed"}>
                <option value="mixed" disabled selected>
                  {tr("mixed")}
                </option>
              </Show>
              <option value="" selected={status() === null}>
                {tr("none")}
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
            <span>{tr("favorite")}</span>
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
              {mixedLabel(favorite(), (v) => (v ? tr("yes") : tr("no")))}
            </button>
          </div>
          <label class="multi__row">
            <span>{tr("type")}</span>
            <select
              class="input"
              onChange={(e) =>
                void changeAll(props.games, {
                  kind: e.currentTarget.value === "app" ? "app" : "game",
                  addCategories: [],
                  removeCategories: [],
                })
              }
            >
              <Show when={kind() === "mixed"}>
                <option value="mixed" disabled selected>
                  {tr("mixed")}
                </option>
              </Show>
              <option value="game" selected={kind() === "game"}>
                {tr("game")}
              </option>
              <option value="app" selected={kind() === "app"}>
                {tr("app")}
              </option>
            </select>
          </label>
          <div class="multi__row">
            <span>{tr("hidden")}</span>
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
              {mixedLabel(hidden(), (v) => (v ? tr("yes") : tr("no")))}
            </button>
          </div>
          <Show when={store.categories().length > 0}>
            <div class="multi__categories" role="group" aria-label={tr("categories")}>
              <span class="muted">{tr("categories")}</span>
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
                            ? tr("takeOffAll", { name: category.name })
                            : tr("addToAll", { name: category.name })
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
              {tr("installAll")}
            </button>
          </Show>
          <button type="button" class="btn" onClick={() => void refreshAll(props.games)}>
            <RefreshCw class="icon" aria-hidden="true" />
            {tr("refreshMetadata")}
          </button>
          <button type="button" class="btn" onClick={() => store.openCombine(ids())}>
            <Combine class="icon" aria-hidden="true" />
            {tr("combineInto")}
          </button>
          <button type="button" class="btn btn--danger" onClick={() => setRemoving(true)}>
            <Trash class="icon" aria-hidden="true" />
            {tr("removeFromLibrary")}
          </button>
        </div>
      </div>
      <Show when={removing()}>
        <ConfirmDialog
          title={tr("removeGamesTitle")}
          message={tr("removeGamesMessage", { count: count() })}
          confirmLabel={tr("remove")}
          onConfirm={() => void removeAll(props.games)}
          onClose={() => setRemoving(false)}
        />
      </Show>
    </aside>
  );
}
