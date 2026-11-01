import ArrowDown from "lucide-solid/icons/arrow-down";
import ArrowUp from "lucide-solid/icons/arrow-up";
import { Index, Show, createSignal } from "solid-js";
import type { SortField } from "../bindings/SortField";
import {
  moveColumn,
  pickerColumns,
  resizeColumn,
  toggleColumn,
  type ShownColumn,
} from "../lib/columns";
import { translator } from "../i18n";
import { libraryTable } from "../i18n/libraryTable";
import { readCssPx } from "../lib/layoutMode";
import { store } from "../lib/store";
import { inSentence } from "../lib/viewOptions";
import type { PopoverAnchor } from "./Popover";
import { SearchPicker, type PickerItem } from "./SearchPicker";

const tr = translator(libraryTable);

/** Fallback for `--qk-column-min` (theme.css). */
const MIN_WIDTH = 48;
/** px per arrow key press on a column edge (Shift: ten times). */
const KEY_STEP = 8;

const NAME_ID = "name";

/** The sort a header sets: the same field again flips the direction. */
function sortBy(field: SortField) {
  if (store.query().sort === field) store.toggleDirection();
  else store.setSort(field);
}

/** A header's ARIA sort state. */
function ariaSort(field: SortField | undefined): "ascending" | "descending" | undefined {
  if (!field || store.query().sort !== field) return undefined;
  return store.query().descending ? "descending" : "ascending";
}

function SortArrow(props: { field: SortField | undefined }) {
  return (
    <Show when={ariaSort(props.field)}>
      {(direction) => (
        <Show
          when={direction() === "descending"}
          fallback={<ArrowUp class="icon list__sort" aria-hidden="true" />}
        >
          <ArrowDown class="icon list__sort" aria-hidden="true" />
        </Show>
      )}
    </Show>
  );
}

/** The column picker's entries: Name (always shown), the shown columns, then the rest. */
const pickerItems = (): PickerItem[] => [
  {
    id: NAME_ID,
    label: tr("name"),
    checked: true,
    disabled: true,
    hint: tr("always"),
    section: tr("shown"),
  },
  ...pickerColumns(store.columns()).map(({ def, shown }) => ({
    id: def.id,
    label: def.label,
    checked: shown,
    movable: shown,
    section: shown ? tr("shown") : tr("moreColumns"),
  })),
];

/** The menu key or Shift+F10 on a header: the column picker under it. */
const isMenuKey = (event: KeyboardEvent) =>
  event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);

/**
 * The table's column header: click a header to sort by it, drag a header to move it, drag its
 * edge (or focus the edge and use the arrow keys) to resize it, right-click (or the menu key)
 * for the column picker.
 */
export function TableHeader(props: { columns: ShownColumn[]; ref?: (el: HTMLDivElement) => void }) {
  const [picker, setPicker] = createSignal<PopoverAnchor | null>(null);
  const [dragged, setDragged] = createSignal<string | null>(null);
  let header: HTMLDivElement | undefined;
  let returnTo: HTMLElement | null = null;

  const minWidth = () => (header ? readCssPx(header, "--qk-column-min", MIN_WIDTH) : MIN_WIDTH);

  function openPicker(anchor: PopoverAnchor) {
    returnTo = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setPicker(anchor);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (!isMenuKey(event)) return;
    event.preventDefault();
    const rect = (event.target as Element).getBoundingClientRect();
    openPicker({ rect, align: "start" });
  }

  function setWidth(id: string, width: number) {
    store.changeColumns((columns) => resizeColumn(columns, id, width, minWidth()));
  }

  /** Dragging a column's edge: the width follows the pointer. */
  function startResize(id: string, event: PointerEvent) {
    const handle = event.currentTarget as HTMLElement;
    const cell = handle.parentElement;
    if (!cell) return;
    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const startWidth = cell.getBoundingClientRect().width;
    handle.setPointerCapture(event.pointerId);
    const onMove = (move: PointerEvent) => setWidth(id, startWidth + move.clientX - startX);
    const onUp = () => {
      handle.removeEventListener("pointermove", onMove);
      handle.removeEventListener("pointerup", onUp);
      handle.removeEventListener("pointercancel", onUp);
    };
    handle.addEventListener("pointermove", onMove);
    handle.addEventListener("pointerup", onUp);
    handle.addEventListener("pointercancel", onUp);
  }

  function resizeByKey(id: string, event: KeyboardEvent) {
    const by = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (by === 0) return;
    event.preventDefault();
    const cell = (event.currentTarget as HTMLElement).parentElement;
    const width = cell?.getBoundingClientRect().width ?? 0;
    setWidth(id, width + by * KEY_STEP * (event.shiftKey ? 10 : 1));
  }

  return (
    <>
      <div
        ref={(el) => {
          header = el;
          props.ref?.(el);
        }}
        class="list__header"
        role="group"
        aria-label={tr("columnsGroup")}
        onContextMenu={(event) => {
          event.preventDefault();
          openPicker({ x: event.clientX, y: event.clientY });
        }}
        onKeyDown={onKeyDown}
      >
        <span />
        <div class="list__th" aria-sort={ariaSort("name")}>
          <button type="button" class="list__th-btn" onClick={() => sortBy("name")}>
            <span class="list__th-label">{tr("name")}</span>
            <SortArrow field="name" />
          </button>
        </div>
        <Index each={props.columns}>
          {(column) => (
            <div
              class="list__th"
              classList={{
                "list__th--numeric": column().def.numeric === true,
                "list__th--dragging": dragged() === column().def.id,
              }}
              aria-sort={ariaSort(column().def.sort)}
              draggable={true}
              onDragStart={(event) => {
                setDragged(column().def.id);
                event.dataTransfer?.setData("text/plain", column().def.id);
              }}
              onDragEnd={() => setDragged(null)}
              onDragOver={(event) => {
                if (dragged() !== null) event.preventDefault();
              }}
              onDrop={(event) => {
                event.preventDefault();
                const from = dragged();
                setDragged(null);
                if (from !== null && from !== column().def.id) {
                  store.changeColumns((columns) => moveColumn(columns, from, column().def.id));
                }
              }}
            >
              <Show
                when={column().def.sort}
                fallback={
                  <span class="list__th-btn list__th-btn--static">
                    <span class="list__th-label">{column().def.label}</span>
                  </span>
                }
              >
                {(field) => (
                  <button
                    type="button"
                    class="list__th-btn"
                    title={tr("sortBy", { column: inSentence(column().def.label) })}
                    onClick={() => sortBy(field())}
                  >
                    <span class="list__th-label">{column().def.label}</span>
                    <SortArrow field={field()} />
                  </button>
                )}
              </Show>
              <span
                class="list__resize"
                role="separator"
                tabindex="0"
                aria-orientation="vertical"
                aria-label={tr("columnWidth", { column: column().def.label })}
                onMouseDown={(event) => event.preventDefault()}
                onPointerDown={(event) => startResize(column().def.id, event)}
                onKeyDown={(event) => resizeByKey(column().def.id, event)}
              />
            </div>
          )}
        </Index>
      </div>
      <Show when={picker()}>
        {(anchor) => (
          <SearchPicker
            anchor={anchor()}
            label={tr("columns")}
            placeholder={tr("findColumn")}
            items={pickerItems()}
            keepOpen
            onPick={(id) => store.changeColumns((columns) => toggleColumn(columns, id))}
            onMove={(id, to) => store.changeColumns((columns) => moveColumn(columns, id, to))}
            onClose={(returnFocus) => {
              setPicker(null);
              if (returnFocus) returnTo?.focus();
            }}
            footer={
              <div class="picker__footer">
                <button
                  type="button"
                  class="btn btn--small btn--ghost"
                  onClick={() => store.changeColumns(() => [])}
                >
                  {tr("resetColumns")}
                </button>
              </div>
            }
          />
        )}
      </Show>
    </>
  );
}
