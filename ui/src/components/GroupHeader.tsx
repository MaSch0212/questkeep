import ChevronRight from "lucide-solid/icons/chevron-right";
import { Show } from "solid-js";
import type { GameGroup } from "../lib/grouping";
import { SourceIcon } from "./SourceIcon";

/** The header row of a group in the grid or a list: click to collapse or expand it. */
export function GroupHeader(props: { group: GameGroup; collapsed: boolean; onToggle: () => void }) {
  const count = () => props.group.games.length;
  return (
    <button
      type="button"
      class="group-header"
      aria-expanded={!props.collapsed}
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
      <span class="group-header__count">
        {count()} {count() === 1 ? "game" : "games"}
      </span>
    </button>
  );
}
