import Copy from "lucide-solid/icons/copy";
import Pencil from "lucide-solid/icons/pencil";
import PencilLine from "lucide-solid/icons/pencil-line";
import Shapes from "lucide-solid/icons/shapes";
import Trash from "lucide-solid/icons/trash";
import { createSignal, For, Show } from "solid-js";
import type { LibraryView } from "../bindings/LibraryView";
import { store } from "../lib/store";
import { IconPicker } from "./IconPicker";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { SidebarEditor } from "./SidebarEditor";
import { ViewIcon } from "./ViewIcon";

interface ContextMenu {
  anchor: PopoverAnchor;
  view: LibraryView | null;
  /** The element to focus again when the menu closes. */
  origin: HTMLElement | null;
}

/** Where a context menu opens: at the pointer, or under the element for the keyboard. */
function contextAnchor(event: MouseEvent, element: HTMLElement): PopoverAnchor {
  // The context-menu key fires the event at (0, 0) or without pointer coordinates.
  if (event.clientX === 0 && event.clientY === 0) {
    return { rect: element.getBoundingClientRect(), align: "start" };
  }
  return { x: event.clientX, y: event.clientY };
}

/**
 * The sidebar: saved views in sections. Picking one applies its filters, sort and layout.
 * Right-click (or the context-menu key) edits a view; "Edit views" switches to edit mode.
 */
export function Sidebar() {
  const [menu, setMenu] = createSignal<ContextMenu | null>(null);
  const [iconFor, setIconFor] = createSignal<{ view: LibraryView; anchor: PopoverAnchor } | null>(
    null,
  );
  // The view whose name to select when edit mode opens from "Rename".
  const [renaming, setRenaming] = createSignal<string | null>(null);

  function startEditing(renameId: string | null) {
    setRenaming(renameId);
    store.setEditingViews(true);
  }

  function entries(target: ContextMenu): MenuEntry[] {
    const view = target.view;
    const edit: MenuEntry = {
      label: "Edit views",
      icon: () => <Pencil class="icon" aria-hidden="true" />,
      onSelect: () => startEditing(null),
    };
    if (!view) return [edit];
    return [
      {
        label: "Rename",
        icon: () => <PencilLine class="icon" aria-hidden="true" />,
        onSelect: () => startEditing(view.id),
      },
      {
        label: "Change icon",
        icon: () => <Shapes class="icon" aria-hidden="true" />,
        onSelect: () => {
          const rect = target.origin?.getBoundingClientRect();
          if (rect) setIconFor({ view, anchor: { rect, align: "start" } });
        },
      },
      {
        label: "Duplicate",
        icon: () => <Copy class="icon" aria-hidden="true" />,
        onSelect: () => void store.duplicateView(view.id),
      },
      {
        label: "Delete view",
        icon: () => <Trash class="icon" aria-hidden="true" />,
        danger: true,
        onSelect: () => void store.deleteView(view.id),
      },
      { kind: "separator" },
      edit,
    ];
  }

  return (
    <nav
      class="sidebar"
      classList={{ "sidebar--editing": store.editingViews() }}
      aria-label="Library views"
      onContextMenu={(event) => {
        if (store.editingViews()) return;
        event.preventDefault();
        const element = event.currentTarget;
        setMenu({ anchor: contextAnchor(event, element), view: null, origin: element });
      }}
    >
      <Show
        when={!store.editingViews()}
        fallback={
          <SidebarEditor renameId={renaming()} onDone={() => store.setEditingViews(false)} />
        }
      >
        <For each={store.sections()}>
          {(section) => (
            <div class="sidebar__group">
              <h2 class="sidebar__heading">{section.name}</h2>
              <ul class="sidebar__list">
                <For each={section.views}>
                  {(view) => {
                    const active = () =>
                      store.page() === "library" && store.activeViewId() === view.id;
                    return (
                      <li>
                        <button
                          type="button"
                          class="nav-item nav-item--icon view-item"
                          classList={{ "nav-item--active": active() }}
                          aria-current={active() ? "page" : undefined}
                          onClick={() => store.selectView(view.id)}
                          onContextMenu={(event) => {
                            event.preventDefault();
                            event.stopPropagation();
                            const element = event.currentTarget;
                            setMenu({
                              anchor: contextAnchor(event, element),
                              view,
                              origin: element,
                            });
                          }}
                        >
                          <ViewIcon name={view.icon} />
                          <span class="nav-item__text">{view.name}</span>
                          <Show when={store.hasDraft(view.id)}>
                            <span class="view-item__unsaved" title="Unsaved changes">
                              <span class="visually-hidden">(unsaved changes)</span>
                            </span>
                          </Show>
                          <span class="view-item__count">{store.viewCounts()[view.id] ?? ""}</span>
                        </button>
                      </li>
                    );
                  }}
                </For>
              </ul>
            </div>
          )}
        </For>
      </Show>
      <Show when={menu()}>
        {(target) => (
          <Menu
            anchor={target().anchor}
            label={target().view ? `${target().view?.name} options` : "Views"}
            entries={entries(target())}
            onClose={(returnFocus) => {
              const origin = target().origin;
              setMenu(null);
              if (returnFocus) origin?.focus();
            }}
          />
        )}
      </Show>
      <Show when={iconFor()}>
        {(target) => (
          <IconPicker
            anchor={target().anchor}
            selected={target().view.icon}
            onPick={(icon) => void store.setViewIcon(target().view.id, icon)}
            onClose={() => setIconFor(null)}
          />
        )}
      </Show>
    </nav>
  );
}
