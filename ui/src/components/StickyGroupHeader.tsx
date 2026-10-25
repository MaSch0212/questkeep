import { Show } from "solid-js";
import { stickyHeader, type GroupedLayout, type HeaderRow } from "../lib/grouping";
import { GroupHeader } from "./GroupHeader";

/**
 * The header of the group scrolled to, pinned to the top of a grouped grid or list; the next
 * header pushes it up. The rows are virtual (absolutely placed), so CSS `sticky` can't pin the
 * real header: this is a copy on a zero-height sticky layer. Keyboard users reach the real one.
 */
export function StickyGroupHeader(props: {
  layout: GroupedLayout;
  scrollTop: number;
  variant: "grid" | "list";
  onToggle: (row: HeaderRow) => void;
}) {
  const pinned = () => stickyHeader(props.layout, props.scrollTop);
  return (
    <div class="sticky-group" aria-hidden="true">
      <Show when={pinned()}>
        {(at) => (
          <div
            class={`sticky-group__bar sticky-group__bar--${props.variant}`}
            style={{
              transform: `translateY(${at().offset}px)`,
              height: `${at().row.height}px`,
            }}
          >
            <div class={props.variant === "grid" ? "grid__group" : "list__group"}>
              <GroupHeader
                group={at().row.group}
                collapsed={at().row.collapsed}
                tabbable={false}
                onToggle={() => props.onToggle(at().row)}
              />
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
