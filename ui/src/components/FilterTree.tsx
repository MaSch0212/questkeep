import Ellipsis from "lucide-solid/icons/ellipsis";
import GripVertical from "lucide-solid/icons/grip-vertical";
import Plus from "lucide-solid/icons/plus";
import X from "lucide-solid/icons/x";
import {
  createContext,
  createSignal,
  createUniqueId,
  For,
  Index,
  Match,
  onMount,
  Show,
  Switch,
  useContext,
  type JSX,
  type ParentProps,
} from "solid-js";
import type { FilterGroup } from "../bindings/FilterGroup";
import type { FilterNode } from "../bindings/FilterNode";
import type { ListFilter } from "../bindings/ListFilter";
import type { ListMode } from "../bindings/ListMode";
import type { MatchMode } from "../bindings/MatchMode";
import type { RangeFilter } from "../bindings/RangeFilter";
import type { TextFilter } from "../bindings/TextFilter";
import type { TextOp } from "../bindings/TextOp";
import { translator } from "../i18n";
import { libraryCommon } from "../i18n/libraryCommon";
import { libraryFilters } from "../i18n/libraryFilters";
import { libraryFilterTree } from "../i18n/libraryFilterTree";
import {
  appendTo,
  canMove,
  type DropPlace,
  moveBy,
  moveChoices,
  moveNode,
  samePath,
  duplicateAt,
  emptyGroup,
  FIELDS,
  fromInput,
  joinerWord,
  listInfo,
  newFilter,
  needsValue,
  NO_VALUE,
  opLabel,
  presetsOf,
  rangeInfo,
  removeAt,
  SECTION_LABELS,
  setMode,
  TEXT_OPS,
  textInfo,
  textPattern,
  toggleNegate,
  toInput,
  ungroupAt,
  updateAt,
  type Path,
} from "../lib/filters";
import { store } from "../lib/store";
import { Menu } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { SearchPicker } from "./SearchPicker";

const tr = translator(libraryFilterTree);
const trCommon = translator(libraryCommon);
const trFilters = translator(libraryFilters);

type Change = (tree: FilterGroup) => FilterGroup;

/** A small segmented control. */
function Segmented<T extends string>(props: {
  label: string;
  value: T;
  options: readonly (readonly [T, string])[];
  onChange: (value: T) => void;
}) {
  return (
    <div class="segmented segmented--sm" role="radiogroup" aria-label={props.label}>
      <For each={props.options}>
        {([value, text]) => (
          <button
            type="button"
            role="radio"
            class="segmented__option"
            classList={{ "segmented__option--on": props.value === value }}
            aria-checked={props.value === value}
            onClick={() => props.onChange(value)}
          >
            {text}
          </button>
        )}
      </For>
    </div>
  );
}

const match = (): readonly (readonly [MatchMode, string])[] => [
  ["all", tr("matchAll")],
  ["any", tr("matchAny")],
];
const listModes = (): readonly (readonly [ListMode, string])[] => [
  ["any", tr("anyOf")],
  ["all", tr("allOf")],
  ["none", tr("noneOf")],
];

const anchorOf = (el: HTMLElement | undefined): PopoverAnchor | null => {
  const rect = el?.getBoundingClientRect();
  return rect ? { rect, align: "start" } : null;
};

/** "+ Add filter" with its field list. */
function AddFilter(props: {
  small?: boolean;
  used: Set<string>;
  onAdd: (node: FilterNode) => void;
}) {
  const [anchor, setAnchor] = createSignal<PopoverAnchor | null>(null);
  let button: HTMLButtonElement | undefined;
  return (
    <>
      <button
        ref={(el) => (button = el)}
        type="button"
        class="add-filter"
        classList={{ "add-filter--sm": props.small }}
        aria-haspopup="dialog"
        aria-expanded={anchor() !== null}
        onClick={() => setAnchor(anchor() ? null : anchorOf(button))}
      >
        <Plus class="icon" aria-hidden="true" />
        {props.small ? tr("filterShort") : tr("addFilter")}
      </button>
      <Show when={anchor()}>
        {(at) => (
          <SearchPicker
            anchor={at()}
            label={tr("addFilter")}
            placeholder={tr("filterBy")}
            // A field can be both a list and a text filter ("Publisher", "Publisher (text)"),
            // so the id is the kind and the field.
            items={FIELDS.map((info) => ({
              id: `${info.kind}:${info.field}`,
              label: info.label,
              section: SECTION_LABELS[info.section],
              hint: props.used.has(`${info.kind}:${info.field}`) ? tr("activeHint") : undefined,
            }))}
            onPick={(id) => {
              const info = FIELDS.find((f) => `${f.kind}:${f.field}` === id);
              if (info) props.onAdd(newFilter(info));
            }}
            onClose={(returnFocus) => {
              setAnchor(null);
              if (returnFocus) button?.focus();
            }}
          />
        )}
      </Show>
    </>
  );
}

function AddGroup(props: { small?: boolean; onAdd: () => void }) {
  return (
    <button
      type="button"
      class="add-filter"
      classList={{ "add-filter--sm": props.small }}
      onClick={() => props.onAdd()}
    >
      <Plus class="icon" aria-hidden="true" />
      {props.small ? tr("groupShort") : tr("addGroup")}
    </button>
  );
}

function RemoveButton(props: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      class="icon-btn icon-btn--small"
      aria-label={props.label}
      data-tooltip={props.label}
      onClick={() => props.onClick()}
    >
      <X class="icon" aria-hidden="true" />
    </button>
  );
}

/** "Not": the row or group then has the games it would leave out. */
function NegateToggle(props: { on: boolean; hint: string; onToggle: () => void }) {
  return (
    <button
      type="button"
      class="filter-not"
      classList={{ "filter-not--on": props.on }}
      aria-pressed={props.on}
      data-tooltip={props.hint}
      onClick={() => props.onToggle()}
    >
      {tr("negate")}
    </button>
  );
}

function rowNegate(props: {
  filter: { negate?: boolean };
  path: Path;
  change: (c: Change) => void;
}) {
  return (
    <NegateToggle
      on={props.filter.negate === true}
      hint={tr("negateHint")}
      onToggle={() => props.change((tree) => toggleNegate(tree, props.path))}
    />
  );
}

function TextRow(props: { filter: TextFilter; path: Path; change: (c: Change) => void }) {
  const info = () => textInfo(props.filter.field);
  const update = (patch: Partial<TextFilter>) =>
    props.change((tree) =>
      updateAt(tree, props.path, (node) => ({ ...node, ...patch }) as FilterNode),
    );
  const invalid = () =>
    props.filter.op === "matches" &&
    props.filter.value !== "" &&
    textPattern(props.filter) === null;
  const hintId = createUniqueId();
  return (
    <div class="filter-row" role="group" aria-label={tr("rowLabel", { label: info().label })}>
      <div class="filter-row__head">
        <MoveHandle path={props.path} label={tr("moveFilter", { label: info().label })} />
        <span class="filter-row__label">{info().label}</span>
        {rowNegate(props)}
        <RemoveButton
          label={tr("removeFilter", { label: info().label })}
          onClick={() => props.change((tree) => removeAt(tree, props.path))}
        />
      </div>
      <div class="filter-row__text">
        <select
          class="filter-row__op"
          aria-label={tr("textOp", { label: info().label })}
          value={props.filter.op}
          onChange={(event) => update({ op: event.currentTarget.value as TextOp })}
        >
          <For each={TEXT_OPS}>{(op) => <option value={op}>{opLabel(op)}</option>}</For>
        </select>
        <Show when={needsValue(props.filter.op)}>
          <input
            type="text"
            class="filter-row__input"
            classList={{ "filter-row__input--invalid": invalid() }}
            aria-label={tr("textValue", { label: info().label })}
            aria-invalid={invalid()}
            aria-describedby={invalid() ? hintId : undefined}
            placeholder={
              props.filter.op === "matches" ? tr("patternPlaceholder") : tr("textPlaceholder")
            }
            spellcheck={false}
            value={props.filter.value}
            onInput={(event) => update({ value: event.currentTarget.value })}
          />
          <button
            type="button"
            class="filter-case"
            classList={{ "filter-case--on": props.filter.caseSensitive === true }}
            aria-pressed={props.filter.caseSensitive === true}
            aria-label={tr("matchCase")}
            data-tooltip={tr("matchCase")}
            onClick={() => update({ caseSensitive: props.filter.caseSensitive !== true })}
          >
            Aa
          </button>
        </Show>
      </div>
      <Show when={invalid()}>
        <p id={hintId} class="filter-row__hint" role="alert">
          {tr("invalidPattern")}
        </p>
      </Show>
    </div>
  );
}

function ListRow(props: { filter: ListFilter; path: Path; change: (c: Change) => void }) {
  const info = () => listInfo(props.filter.field);
  const [anchor, setAnchor] = createSignal<PopoverAnchor | null>(null);
  let addButton: HTMLButtonElement | undefined;
  const update = (patch: Partial<ListFilter>) =>
    props.change((tree) =>
      updateAt(tree, props.path, (node) => ({ ...node, ...patch }) as FilterNode),
    );
  const toggle = (key: string) =>
    update({
      values: props.filter.values.includes(key)
        ? props.filter.values.filter((v) => v !== key)
        : [...props.filter.values, key],
    });
  const name = (key: string) =>
    key === NO_VALUE
      ? trFilters("noValue", { plural: info().plural })
      : store.valueName(props.filter.field, key);

  onMount(() => void store.loadValues(props.filter.field));

  return (
    <div class="filter-row" role="group" aria-label={tr("rowLabel", { label: info().label })}>
      <div class="filter-row__head">
        <MoveHandle path={props.path} label={tr("moveFilter", { label: info().label })} />
        <span class="filter-row__label">{info().label}</span>
        <Segmented
          label={tr("howCombine", { plural: info().plural })}
          value={props.filter.mode}
          options={listModes()}
          onChange={(mode) => update({ mode })}
        />
        {rowNegate(props)}
        <RemoveButton
          label={tr("removeFilter", { label: info().label })}
          onClick={() => props.change((tree) => removeAt(tree, props.path))}
        />
      </div>
      <div class="filter-row__values">
        <For each={props.filter.values}>
          {(key) => (
            <span class="value-chip" classList={{ "value-chip--none": key === NO_VALUE }}>
              {name(key)}
              <button
                type="button"
                class="value-chip__remove"
                aria-label={tr("removeValue", { name: name(key) })}
                onClick={() => toggle(key)}
              >
                <X class="icon" aria-hidden="true" />
              </button>
            </span>
          )}
        </For>
        <button
          ref={(el) => (addButton = el)}
          type="button"
          class="value-chip value-chip--add"
          aria-haspopup="dialog"
          aria-expanded={anchor() !== null}
          onClick={() => {
            void store.loadValues(props.filter.field);
            setAnchor(anchor() ? null : anchorOf(addButton));
          }}
        >
          <Plus class="icon" aria-hidden="true" />
          {tr("addNoun", { noun: info().noun })}
        </button>
      </div>
      <Show when={anchor()}>
        {(at) => (
          <SearchPicker
            anchor={at()}
            label={tr("valuesLabel", { label: info().label })}
            placeholder={tr("searchPlural", { plural: info().plural })}
            keepOpen
            empty={
              store.valueLists()[props.filter.field] === undefined
                ? tr("loading")
                : trCommon("nothingFound")
            }
            items={[
              ...(store.valueLists()[props.filter.field] ?? []).map((value) => ({
                id: value.key,
                // Store ids, category and status ids show by their names.
                label: name(value.key),
                hint: String(value.count),
                checked: props.filter.values.includes(value.key),
              })),
              ...(props.filter.field === "source"
                ? []
                : [
                    {
                      id: NO_VALUE,
                      label: trFilters("noValue", { plural: info().plural }),
                      section: tr("special"),
                      special: true,
                      checked: props.filter.values.includes(NO_VALUE),
                    },
                  ]),
            ]}
            onPick={toggle}
            onClose={(returnFocus) => {
              setAnchor(null);
              if (returnFocus) addButton?.focus();
            }}
          />
        )}
      </Show>
    </div>
  );
}

function RangeRow(props: { filter: RangeFilter; path: Path; change: (c: Change) => void }) {
  const info = () => rangeInfo(props.filter.field);
  const set = (min: number | null, max: number | null) =>
    props.change((tree) =>
      updateAt(tree, props.path, (node) => ({ ...node, min, max }) as FilterNode),
    );
  const inputType = () => (info().unit === "date" ? "date" : "text");
  const suffix = () => ({ hours: "h", gigabytes: "GB" })[info().unit as string] ?? "";
  const bound = (which: "min" | "max"): JSX.Element => (
    <span class="range-input">
      <input
        type={inputType()}
        inputmode={info().unit === "date" ? undefined : "decimal"}
        class="range-input__field"
        aria-label={tr(which === "min" ? "rangeFrom" : "rangeTo", { label: info().label })}
        placeholder={which === "min" ? tr("min") : tr("max")}
        value={toInput(info().unit, props.filter[which])}
        onChange={(event) => {
          const value = fromInput(info().unit, event.currentTarget.value);
          if (which === "min") set(value, props.filter.max);
          else set(props.filter.min, value);
        }}
      />
      <Show when={suffix()}>
        <span class="range-input__unit">{suffix()}</span>
      </Show>
    </span>
  );
  return (
    <div class="filter-row" role="group" aria-label={tr("rowLabel", { label: info().label })}>
      <div class="filter-row__head">
        <MoveHandle path={props.path} label={tr("moveFilter", { label: info().label })} />
        <span class="filter-row__label">{info().label}</span>
        {rowNegate(props)}
        <RemoveButton
          label={tr("removeFilter", { label: info().label })}
          onClick={() => props.change((tree) => removeAt(tree, props.path))}
        />
      </div>
      <div class="filter-row__range">
        {bound("min")}
        <span class="filter-row__to">{tr("to")}</span>
        {bound("max")}
        <For each={presetsOf(info())}>
          {(preset) => (
            <button
              type="button"
              class="value-chip value-chip--preset"
              classList={{
                "value-chip--on":
                  preset.min === props.filter.min && preset.max === props.filter.max,
              }}
              onClick={() => set(preset.min, preset.max)}
            >
              {preset.label}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}

const asList = (n: FilterNode) => (n.kind === "list" ? n : undefined);
const asRange = (n: FilterNode) => (n.kind === "range" ? n : undefined);
const asText = (n: FilterNode) => (n.kind === "text" ? n : undefined);
const asGroup = (n: FilterNode) => (n.kind === "group" ? n : undefined);

/** The used fields of a tree (for "active" in the field list). */
function usedFields(group: FilterGroup, into = new Set<string>()): Set<string> {
  for (const node of group.items) {
    if (node.kind === "group") usedFields(node, into);
    else into.add(`${node.kind}:${node.field}`);
  }
  return into;
}

/** The items of a group with "and"/"or" between them, and its add buttons. */
function GroupItems(props: {
  group: FilterGroup;
  path: Path;
  root: FilterGroup;
  change: (c: Change) => void;
  small?: boolean;
}) {
  const drag = useDrag();
  return (
    <>
      {/* By index, so a change keeps the rows (and focus, open pickers) in place. */}
      <Index each={props.group.items}>
        {(node, index) => (
          <>
            <Show when={index > 0}>
              <span
                class="filter-joiner"
                classList={{ "filter-joiner--or": props.group.mode === "any" }}
              >
                {joinerWord(props.group.mode === "all" ? "and" : "or")}
              </span>
            </Show>
            <DropItem path={[...props.path, index]} group={node().kind === "group"}>
              <Switch>
                <Match when={asList(node())}>
                  {(list) => (
                    <ListRow filter={list()} path={[...props.path, index]} change={props.change} />
                  )}
                </Match>
                <Match when={asRange(node())}>
                  {(range) => (
                    <RangeRow
                      filter={range()}
                      path={[...props.path, index]}
                      change={props.change}
                    />
                  )}
                </Match>
                <Match when={asText(node())}>
                  {(text) => (
                    <TextRow filter={text()} path={[...props.path, index]} change={props.change} />
                  )}
                </Match>
                <Match when={asGroup(node())}>
                  {(group) => (
                    <GroupFrame
                      group={group()}
                      path={[...props.path, index]}
                      root={props.root}
                      change={props.change}
                    />
                  )}
                </Match>
              </Switch>
            </DropItem>
          </>
        )}
      </Index>
      {/* Dropping here puts the dragged filter at the end of this group. */}
      <div
        class="filter-tree__adds"
        classList={{ "filter-tree__adds--drop": dragIsOver(drag, props.path, "into") }}
        onDragOver={(event) => dragOver(drag, event, props.path, "into")}
        onDrop={(event) => dropHere(drag, event)}
      >
        <AddFilter
          small={props.small}
          used={usedFields(props.root)}
          onAdd={(node) => props.change((tree) => appendTo(tree, props.path, node))}
        />
        <AddGroup
          small={props.small}
          onAdd={() =>
            props.change((tree) => appendTo(tree, props.path, { kind: "group", ...emptyGroup() }))
          }
        />
      </div>
    </>
  );
}

function GroupFrame(props: {
  group: FilterGroup;
  path: Path;
  root: FilterGroup;
  change: (c: Change) => void;
}) {
  const [menu, setMenu] = createSignal<PopoverAnchor | null>(null);
  let more: HTMLButtonElement | undefined;
  return (
    <div class="filter-group" role="group" aria-label={tr("filterGroup")}>
      <div class="filter-group__head">
        <MoveHandle path={props.path} label={tr("moveGroup")} />
        <Segmented
          label={tr("groupMatches")}
          value={props.group.mode}
          options={[
            ["all", tr("groupAll")],
            ["any", tr("groupAny")],
          ]}
          onChange={(mode) => props.change((tree) => setMode(tree, props.path, mode))}
        />
        <span class="filter-group__caption">{tr("ofThisGroup")}</span>
        <span class="filter-group__spacer" />
        <NegateToggle
          on={props.group.negate === true}
          hint={tr("negateGroupHint")}
          onToggle={() => props.change((tree) => toggleNegate(tree, props.path))}
        />
        <button
          ref={(el) => (more = el)}
          type="button"
          class="icon-btn icon-btn--small"
          aria-label={tr("groupActions")}
          data-tooltip={tr("groupActions")}
          aria-haspopup="menu"
          aria-expanded={menu() !== null}
          onClick={() => setMenu(menu() ? null : anchorOf(more))}
        >
          <Ellipsis class="icon" aria-hidden="true" />
        </button>
        <RemoveButton
          label={tr("removeGroup")}
          onClick={() => props.change((tree) => removeAt(tree, props.path))}
        />
      </div>
      <GroupItems
        group={props.group}
        path={props.path}
        root={props.root}
        change={props.change}
        small
      />
      <Show when={menu()}>
        {(at) => (
          <Menu
            anchor={at()}
            label={tr("groupActions")}
            inside={() => [more]}
            onClose={(returnFocus) => {
              setMenu(null);
              if (returnFocus) more?.focus();
            }}
            entries={[
              {
                label: tr("ungroup"),
                onSelect: () => props.change((tree) => ungroupAt(tree, props.path)),
              },
              {
                label: tr("duplicate"),
                onSelect: () => props.change((tree) => duplicateAt(tree, props.path)),
              },
            ]}
          />
        )}
      </Show>
    </div>
  );
}

/** The added filters of a view: "Games must match all/any of these", rows and nested groups. */
export function FilterTree(props: { tree: FilterGroup; onChange: (tree: FilterGroup) => void }) {
  const change = (c: Change) => props.onChange(c(props.tree));
  const [from, setFrom] = createSignal<Path | null>(null);
  const [over, setOver] = createSignal<Drop | null>(null);
  const drag: DragApi = {
    tree: () => props.tree,
    from,
    over,
    start: (path) => {
      setFrom(path);
      setOver(null);
    },
    hover: setOver,
    drop: () => {
      const source = from();
      const target = over();
      setFrom(null);
      setOver(null);
      if (source && target) change((tree) => moveNode(tree, source, target.path, target.place));
    },
    end: () => {
      setFrom(null);
      setOver(null);
    },
    move: (path, how) => change((tree) => moveBy(tree, path, how)),
  };
  return (
    <DragContext.Provider value={drag}>
      <FilterTreeBody tree={props.tree} change={change} />
    </DragContext.Provider>
  );
}

function FilterTreeBody(props: { tree: FilterGroup; change: (c: Change) => void }) {
  const drag = useDrag();
  const change = props.change;
  return (
    <div
      class="filter-tree"
      // Anywhere not over a row, group or add area: no drop.
      onDragOver={() => {
        if (drag.from()) drag.hover(null);
      }}
    >
      <div class="filter-tree__match">
        {tr("gamesMustMatch")}
        <Segmented
          label={tr("gamesMustMatch")}
          value={props.tree.mode}
          options={match()}
          onChange={(mode) => change((tree) => setMode(tree, [], mode))}
        />
        {tr("ofThese")}
      </div>
      <GroupItems group={props.tree} path={[]} root={props.tree} change={change} />
    </div>
  );
}

// ---- drag and drop ----

interface Drop {
  path: Path;
  place: DropPlace;
}

interface DragApi {
  tree: () => FilterGroup;
  /** The dragged node. */
  from: () => Path | null;
  /** Where it would land. */
  over: () => Drop | null;
  start: (path: Path) => void;
  hover: (drop: Drop | null) => void;
  drop: () => void;
  end: () => void;
  /** A move from the handle's menu. */
  move: (path: Path, how: MoveHow) => void;
}

const DragContext = createContext<DragApi>();

function useDrag(): DragApi {
  const drag = useContext(DragContext);
  if (!drag) throw new Error("filter rows need a FilterTree");
  return drag;
}

/** Whether the pending drop is `place` of `path` (for its marker). */
function dragIsOver(drag: DragApi, path: Path, place: DropPlace): boolean {
  const at = drag.over();
  return drag.from() !== null && at !== null && at.place === place && samePath(at.path, path);
}

/** Accepts a drag over an element as `place` of `path`, when that move is allowed. */
function dragOver(drag: DragApi, event: DragEvent, path: Path, place: DropPlace) {
  const source = drag.from();
  if (!source) return;
  event.stopPropagation();
  if (!canMove(drag.tree(), source, path, place)) {
    drag.hover(null);
    return;
  }
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  const at = drag.over();
  if (!at || at.place !== place || !samePath(at.path, path)) drag.hover({ path, place });
}

function dropHere(drag: DragApi, event: DragEvent) {
  if (!drag.from()) return;
  event.preventDefault();
  event.stopPropagation();
  drag.drop();
}

/** Pixels at a group's bottom edge that drop after it instead of into it. */
const GROUP_AFTER_EDGE = 10;

/** A row or group as a drop target: before or after it, or (a group) into it. */
function DropItem(props: ParentProps<{ path: Path; group: boolean }>) {
  const drag = useDrag();
  let el: HTMLDivElement | undefined;
  const placeAt = (event: DragEvent): DropPlace => {
    const rect = el?.getBoundingClientRect();
    if (!rect) return "after";
    if (props.group) {
      const head = el?.querySelector(".filter-group__head")?.getBoundingClientRect();
      if (head && event.clientY < head.bottom) return "before";
      return event.clientY > rect.bottom - GROUP_AFTER_EDGE ? "after" : "into";
    }
    return event.clientY < rect.top + rect.height / 2 ? "before" : "after";
  };
  const dragging = () => {
    const source = drag.from();
    return source !== null && samePath(source, props.path);
  };
  return (
    <div
      ref={(node) => (el = node)}
      class="filter-item"
      classList={{
        "filter-item--before": dragIsOver(drag, props.path, "before"),
        "filter-item--after": dragIsOver(drag, props.path, "after"),
        "filter-item--into": props.group && dragIsOver(drag, props.path, "into"),
        "filter-item--dragging": dragging(),
      }}
      onDragOver={(event) => dragOver(drag, event, props.path, placeAt(event))}
      onDrop={(event) => dropHere(drag, event)}
    >
      {props.children}
    </div>
  );
}

type MoveHow = "up" | "down" | "out" | "intoAbove";

/** The grip of a row or group: drag it to move, or click it for a menu of moves. */
function MoveHandle(props: { path: Path; label: string }) {
  const drag = useDrag();
  const [menu, setMenu] = createSignal<PopoverAnchor | null>(null);
  let button: HTMLButtonElement | undefined;
  const entries = () => {
    const can = moveChoices(drag.tree(), props.path);
    const entry = (how: MoveHow, label: string) => ({
      label,
      disabled: !can[how],
      onSelect: () => drag.move(props.path, how),
    });
    return [
      entry("up", tr("moveUp")),
      entry("down", tr("moveDown")),
      entry("out", tr("moveOut")),
      entry("intoAbove", tr("moveIntoAbove")),
    ];
  };
  return (
    <>
      <button
        ref={(node) => (button = node)}
        type="button"
        class="filter-handle"
        draggable="true"
        aria-label={props.label}
        data-tooltip={tr("dragHint")}
        aria-haspopup="menu"
        aria-expanded={menu() !== null}
        onDragStart={(event) => {
          event.stopPropagation();
          const item = button?.closest<HTMLElement>(".filter-item");
          if (event.dataTransfer) {
            event.dataTransfer.effectAllowed = "move";
            event.dataTransfer.setData("text/plain", props.label);
            // The whole row follows the pointer, not just the grip.
            const rect = item?.getBoundingClientRect();
            if (item && rect) {
              event.dataTransfer.setDragImage(
                item,
                event.clientX - rect.left,
                event.clientY - rect.top,
              );
            }
          }
          drag.start(props.path);
        }}
        onDragEnd={() => drag.end()}
        onClick={() => setMenu(menu() ? null : anchorOf(button))}
      >
        <GripVertical class="icon" aria-hidden="true" />
      </button>
      <Show when={menu()}>
        {(at) => (
          <Menu
            anchor={at()}
            label={props.label}
            inside={() => [button]}
            onClose={(returnFocus) => {
              setMenu(null);
              if (returnFocus) button?.focus();
            }}
            entries={entries()}
          />
        )}
      </Show>
    </>
  );
}
