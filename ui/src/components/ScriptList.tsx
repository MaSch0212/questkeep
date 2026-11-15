import Ellipsis from "lucide-solid/icons/ellipsis";
import GripVertical from "lucide-solid/icons/grip-vertical";
import Pencil from "lucide-solid/icons/pencil";
import { createEffect, createSignal, For, type JSX, on, Show } from "solid-js";
import type { GameQuery } from "../bindings/GameQuery";
import type { RuntimeInfo } from "../bindings/RuntimeInfo";
import type { Script } from "../bindings/Script";
import { translator } from "../i18n";
import { scriptsText } from "../i18n/scripts";
import { filterSummary, ownSummary, runsText, scopeText } from "../lib/scripts";
import { store } from "../lib/store";

const tr = translator(scriptsText);

/** The filters of a script as short text ("Installed, Genre is Racing +1"). */
export function filterText(query: GameQuery | null): string | null {
  if (query === null) return null;
  return filterSummary(
    query,
    store.valueName,
    (id) => store.valueName("source", id),
    (id) => store.valueName("category", id),
  );
}

/** One row of a script list: a script, or (pre and post) the fixed place of games' own scripts. */
export type ListRow =
  | { key: string; kind: "script"; script: Script }
  | { key: string; kind: "placeholder"; title: string; hint: string };

/**
 * A reorderable list of rows. Rows move by drag and drop, or by Alt+Up / Alt+Down on a focused
 * row (and the callers' "Move up" / "Move down" menu entries). `row` draws what follows the grip.
 * The caller owns the order: `onMove` and `onNudge` ask for a change, the list follows `rows`.
 */
export function ScriptList(props: {
  label: string;
  rows: ListRow[];
  /** Whether rows can be moved (a single row, or a list that is saved elsewhere, cannot). */
  reorderable: boolean;
  /** A row was dropped on the place of another one. */
  onMove: (from: string, to: string) => void;
  /** A row moves one place up or down. */
  onNudge: (key: string, by: -1 | 1) => void;
  row: (row: ListRow) => JSX.Element;
}) {
  const [dragged, setDragged] = createSignal<string | null>(null);
  const [over, setOver] = createSignal<string | null>(null);
  const [announcement, setAnnouncement] = createSignal("");
  let list: HTMLUListElement | undefined;
  /** The row that keeps the focus across the reload that follows a move. */
  let refocus: string | null = null;

  createEffect(
    on(
      () => props.rows,
      () => {
        const key = refocus;
        if (key === null) return;
        refocus = null;
        queueMicrotask(() => {
          const rows = Array.from(list?.querySelectorAll<HTMLElement>("[data-row-key]") ?? []);
          rows.find((el) => el.dataset.rowKey === key)?.focus();
        });
      },
      { defer: true },
    ),
  );

  const title = (row: ListRow) => (row.kind === "script" ? row.script.name : row.title);

  function nudge(row: ListRow, by: -1 | 1) {
    const index = props.rows.findIndex((r) => r.key === row.key);
    const target = index + by;
    if (target < 0 || target >= props.rows.length) return;
    refocus = row.key;
    setAnnouncement(
      tr("moved", { name: title(row), position: target + 1, total: props.rows.length }),
    );
    props.onNudge(row.key, by);
  }

  return (
    // Positioned, so the screen-reader-only texts (absolutely positioned) stay inside the list's
    // scroll container instead of stretching the whole window.
    <div class="script-list-box">
      <ul ref={(el) => (list = el)} class="script-list" aria-label={props.label}>
        <For each={props.rows}>
          {(row) => (
            <li
              class="script-row"
              classList={{
                "script-row--placeholder": row.kind === "placeholder",
                "script-row--off": row.kind === "script" && !row.script.enabled,
                "script-row--dragging": dragged() === row.key,
                "script-row--over": over() === row.key && dragged() !== row.key,
              }}
              data-row-key={row.key}
              tabindex="0"
              draggable={props.reorderable}
              onDragStart={(event) => {
                setDragged(row.key);
                event.dataTransfer?.setData("text/plain", row.key);
              }}
              onDragEnd={() => {
                setDragged(null);
                setOver(null);
              }}
              onDragOver={(event) => {
                if (dragged() === null) return;
                event.preventDefault();
                setOver(row.key);
              }}
              onDrop={(event) => {
                event.preventDefault();
                const from = dragged();
                setDragged(null);
                setOver(null);
                if (from !== null && from !== row.key) props.onMove(from, row.key);
              }}
              onKeyDown={(event) => {
                if (!props.reorderable || !event.altKey) return;
                if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
                event.preventDefault();
                nudge(row, event.key === "ArrowUp" ? -1 : 1);
              }}
            >
              <span
                class="script-row__grip"
                classList={{ "script-row__grip--off": !props.reorderable }}
                data-tooltip={props.reorderable ? tr("dragTooltip") : undefined}
              >
                <GripVertical class="icon" aria-hidden="true" />
                <span class="visually-hidden">{tr("dragHandle", { name: title(row) })}</span>
              </span>
              {props.row(row)}
            </li>
          )}
        </For>
      </ul>
      <p class="visually-hidden" role="status" aria-live="polite">
        {announcement()}
      </p>
    </div>
  );
}

/**
 * What follows the grip on a script's row: the enable switch, the one-line summary, the scope
 * badge and the Edit and ⋯ buttons.
 */
export function ScriptRowContent(props: {
  script: Script;
  runtimes: readonly RuntimeInfo[];
  /** The summary of a game's own script starts with its kind. */
  own?: boolean;
  onToggle: (enabled: boolean) => void;
  onEdit: () => void;
  onMore: (anchor: HTMLElement) => void;
}) {
  const filter = () => filterText(props.script.filter);
  const summary = () =>
    props.own
      ? ownSummary(props.script, props.runtimes)
      : `${scopeText(props.script, filter())} · ${runsText(props.script, props.runtimes)}`;
  const badge = () =>
    props.script.gameId !== null
      ? null
      : props.script.filter === null
        ? tr("badgeGlobal")
        : tr("badgeFilter");
  return (
    <>
      <input
        type="checkbox"
        class="script-switch"
        role="switch"
        aria-label={tr("toggleScript", { name: props.script.name })}
        checked={props.script.enabled}
        onChange={(event) => props.onToggle(event.currentTarget.checked)}
      />
      <div class="script-row__main">
        <b class="script-row__name">{props.script.name}</b>
        <small class="script-row__summary">{summary()}</small>
      </div>
      <Show when={!props.own && badge()}>
        <span
          class="script-badge"
          classList={{ "script-badge--filter": props.script.filter !== null }}
        >
          {badge()}
        </span>
      </Show>
      <button
        type="button"
        class="icon-btn"
        aria-label={`${tr("edit")}: ${props.script.name}`}
        data-tooltip={tr("edit")}
        onClick={() => props.onEdit()}
      >
        <Pencil class="icon" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="icon-btn"
        aria-label={`${tr("more")}: ${props.script.name}`}
        aria-haspopup="menu"
        data-tooltip={tr("more")}
        onClick={(event) => props.onMore(event.currentTarget)}
      >
        <Ellipsis class="icon" aria-hidden="true" />
      </button>
    </>
  );
}
