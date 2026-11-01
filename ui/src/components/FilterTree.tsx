import Ellipsis from "lucide-solid/icons/ellipsis";
import Plus from "lucide-solid/icons/plus";
import X from "lucide-solid/icons/x";
import { createSignal, For, Index, Match, onMount, Show, Switch, type JSX } from "solid-js";
import type { FilterGroup } from "../bindings/FilterGroup";
import type { FilterNode } from "../bindings/FilterNode";
import type { ListFilter } from "../bindings/ListFilter";
import type { ListMode } from "../bindings/ListMode";
import type { MatchMode } from "../bindings/MatchMode";
import type { RangeFilter } from "../bindings/RangeFilter";
import { translator } from "../i18n";
import { libraryCommon } from "../i18n/libraryCommon";
import { libraryFilters } from "../i18n/libraryFilters";
import { libraryFilterTree } from "../i18n/libraryFilterTree";
import {
  appendTo,
  duplicateAt,
  emptyGroup,
  FIELDS,
  fromInput,
  joinerWord,
  listInfo,
  newFilter,
  NO_VALUE,
  presetsOf,
  rangeInfo,
  removeAt,
  SECTION_LABELS,
  setMode,
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
            items={FIELDS.map((info) => ({
              id: info.field,
              label: info.label,
              section: SECTION_LABELS[info.section],
              hint: props.used.has(info.field) ? tr("activeHint") : undefined,
            }))}
            onPick={(id) => {
              const info = FIELDS.find((f) => f.field === id);
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
      title={props.label}
      onClick={() => props.onClick()}
    >
      <X class="icon" aria-hidden="true" />
    </button>
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
        <span class="filter-row__label">{info().label}</span>
        <Segmented
          label={tr("howCombine", { plural: info().plural })}
          value={props.filter.mode}
          options={listModes()}
          onChange={(mode) => update({ mode })}
        />
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
    <div class="filter-row" role="group" aria-label={`${info().label} filter`}>
      <div class="filter-row__head">
        <span class="filter-row__label">{info().label}</span>
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
const asGroup = (n: FilterNode) => (n.kind === "group" ? n : undefined);

/** The used fields of a tree (for "active" in the field list). */
function usedFields(group: FilterGroup, into = new Set<string>()): Set<string> {
  for (const node of group.items) {
    if (node.kind === "group") usedFields(node, into);
    else into.add(node.field);
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
            <Switch>
              <Match when={asList(node())}>
                {(list) => (
                  <ListRow filter={list()} path={[...props.path, index]} change={props.change} />
                )}
              </Match>
              <Match when={asRange(node())}>
                {(range) => (
                  <RangeRow filter={range()} path={[...props.path, index]} change={props.change} />
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
          </>
        )}
      </Index>
      <div class="filter-tree__adds">
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
        <button
          ref={(el) => (more = el)}
          type="button"
          class="icon-btn icon-btn--small"
          aria-label={tr("groupActions")}
          title={tr("groupActions")}
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
  return (
    <div class="filter-tree">
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
