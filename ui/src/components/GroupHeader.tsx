import ChevronRight from "lucide-solid/icons/chevron-right";
import { Show } from "solid-js";
import { store } from "../lib/store";
import { groupSummary, type GameGroup } from "../lib/grouping";
import { SourceIcon } from "./SourceIcon";

/**
 * The header band of a group in the grid or a list: chevron, store icon (grouped by store),
 * name, a count chip and a short summary. Click to collapse or expand the group.
 */
export function GroupHeader(props: {
  group: GameGroup;
  collapsed: boolean;
  onToggle: () => void;
  /** False for the pinned copy of a header, so Tab only reaches the real one. */
  tabbable?: boolean;
}) {
  const count = () => props.group.games.length;
  /** How many of the group's games a multi-selection holds (0 without one). */
  const picked = () =>
    store.isMulti() ? props.group.games.filter((g) => store.isSelected(g.id)).length : 0;
  return (
    <button
      type="button"
      class="group-header"
      tabindex={props.tabbable === false ? -1 : undefined}
      aria-expanded={!props.collapsed}
      aria-label={`${props.group.label}, ${count()} ${count() === 1 ? "game" : "games"}`}
      title={props.collapsed ? "Show this group" : "Collapse this group"}
      onClick={() => props.onToggle()}
    >
      <ChevronRight
        class="icon group-header__chevron"
        classList={{ "group-header__chevron--open": !props.collapsed }}
        aria-hidden="true"
      />
      <Show when={props.group.source}>{(source) => <SourceIcon source={source()} />}</Show>
      <span class="group-header__label">{props.group.label}</span>
      <span class="group-header__count" aria-hidden="true">
        {count()}
      </span>
      <span class="group-header__meta" aria-hidden="true">
        {picked() > 0
          ? `${picked()} of ${count()} selected`
          : props.collapsed
            ? "Collapsed"
            : groupSummary(props.group.games)}
      </span>
    </button>
  );
}
