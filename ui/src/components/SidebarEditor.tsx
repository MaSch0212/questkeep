import GripVertical from "lucide-solid/icons/grip-vertical";
import Plus from "lucide-solid/icons/plus";
import Trash from "lucide-solid/icons/trash";
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import type { LibraryView } from "../bindings/LibraryView";
import type { ViewSection } from "../bindings/ViewSection";
import { store } from "../lib/store";
import {
  applyDrop,
  dropTargetAt,
  nudgeSection,
  nudgeView,
  type DropTarget,
  type EditRow,
} from "../lib/views";
import { IconPicker } from "./IconPicker";
import type { PopoverAnchor } from "./Popover";
import { ViewIcon } from "./ViewIcon";

type Dragged = { kind: "section" | "view"; id: string };

/** A name field that saves on Enter or when it loses focus, and reverts on Escape. */
function NameInput(props: {
  value: string;
  label: string;
  class?: string;
  ref?: (el: HTMLInputElement) => void;
  onSave: (name: string) => void;
}) {
  let input: HTMLInputElement | undefined;
  const save = () => {
    const name = input?.value.trim() ?? "";
    if (name === "" || name === props.value) {
      if (input) input.value = props.value;
      return;
    }
    props.onSave(name);
  };
  return (
    <input
      ref={(el) => {
        input = el;
        props.ref?.(el);
      }}
      class={`edit-row__name ${props.class ?? ""}`}
      value={props.value}
      aria-label={props.label}
      spellcheck={false}
      onBlur={save}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          save();
        } else if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          if (input) input.value = props.value;
        }
      }}
    />
  );
}

/**
 * The sidebar in edit mode: drag the handles (or press Alt+Up/Down) to reorder views and
 * sections, rename in place, change icons, delete, add sections. Every change is saved at once.
 */
export function SidebarEditor(props: { renameId: string | null; onDone: () => void }) {
  const [dragged, setDragged] = createSignal<Dragged | null>(null);
  const [drop, setDrop] = createSignal<DropTarget | null>(null);
  const [confirmSection, setConfirmSection] = createSignal<string | null>(null);
  const [iconFor, setIconFor] = createSignal<{ view: LibraryView; anchor: PopoverAnchor } | null>(
    null,
  );
  const nameInputs = new Map<string, HTMLInputElement>();
  let list: HTMLDivElement | undefined;
  let focusAfterLoad: string | null = null;

  onMount(() => {
    const id = props.renameId;
    queueMicrotask(() => {
      const input = id ? nameInputs.get(id) : undefined;
      input?.focus();
      input?.select();
    });
  });

  function rows(): EditRow[] {
    if (!list) return [];
    return [...list.querySelectorAll<HTMLElement>("[data-edit-row]")].map((el) => {
      const rect = el.getBoundingClientRect();
      return {
        kind: el.dataset.kind === "section" ? "section" : "view",
        sectionId: el.dataset.section ?? "",
        index: Number(el.dataset.index),
        middle: rect.top + rect.height / 2,
      };
    });
  }

  function startDrag(event: PointerEvent, item: Dragged) {
    if (event.button !== 0) return;
    event.preventDefault();
    setDragged(item);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp, { once: true });
  }

  function onPointerMove(event: PointerEvent) {
    const item = dragged();
    if (!item) return;
    const sections = store.sections();
    setDrop(
      dropTargetAt(
        rows(),
        sections.map((s) => s.id),
        Object.fromEntries(sections.map((s) => [s.id, s.views.length])),
        item.kind,
        event.clientY,
      ),
    );
  }

  function onPointerUp() {
    window.removeEventListener("pointermove", onPointerMove);
    const item = dragged();
    const target = drop();
    setDragged(null);
    setDrop(null);
    if (item && target) void store.arrangeViews(applyDrop(store.sections(), item, target));
  }

  onCleanup(() => {
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
  });

  /** Alt+Up/Down moves the row; focus stays on the moved row's name. */
  function onRowKeyDown(event: KeyboardEvent, item: Dragged) {
    if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
    event.preventDefault();
    const direction = event.key === "ArrowUp" ? -1 : 1;
    const sections = store.sections();
    const next =
      item.kind === "view"
        ? nudgeView(sections, item.id, direction)
        : nudgeSection(sections, item.id, direction);
    void store.arrangeViews(next).then(() => nameInputs.get(item.id)?.focus());
  }

  const showsLine = (target: DropTarget) => {
    const current = drop();
    if (!current || current.kind !== target.kind) return false;
    if (current.kind === "section" && target.kind === "section") {
      return current.index === target.index;
    }
    return (
      current.kind === "view" &&
      target.kind === "view" &&
      current.sectionId === target.sectionId &&
      current.index === target.index
    );
  };

  async function addSection() {
    const id = await store.createSection("New section");
    if (id) {
      focusAfterLoad = id;
      queueMicrotask(() => {
        const input = focusAfterLoad ? nameInputs.get(focusAfterLoad) : undefined;
        input?.focus();
        input?.select();
        focusAfterLoad = null;
      });
    }
  }

  function deleteSection(section: ViewSection) {
    if (section.views.length > 0 && confirmSection() !== section.id) {
      setConfirmSection(section.id);
      return;
    }
    setConfirmSection(null);
    void store.deleteSection(section.id);
  }

  const handle = (label: string, item: () => Dragged) => (
    <span
      class="edit-row__handle"
      title={`Drag to move (or Alt+Up/Down)`}
      aria-hidden="true"
      onPointerDown={(event) => startDrag(event, item())}
    >
      <GripVertical class="icon" />
      <span class="visually-hidden">{label}</span>
    </span>
  );

  return (
    <div class="sidebar-editor" classList={{ "sidebar-editor--dragging": dragged() !== null }}>
      <div class="sidebar-editor__bar">
        <span>Editing views</span>
        <button type="button" class="btn btn--primary btn--small" onClick={() => props.onDone()}>
          Done
        </button>
      </div>
      <div class="sidebar-editor__list" ref={(el) => (list = el)}>
        <For each={store.sections()}>
          {(section, sectionIndex) => (
            <>
              <Show when={showsLine({ kind: "section", index: sectionIndex() })}>
                <div class="edit-row__drop" />
              </Show>
              <div
                class="edit-row edit-row--section"
                classList={{ "edit-row--dragged": dragged()?.id === section.id }}
                data-edit-row
                data-kind="section"
                data-section={section.id}
                data-index={sectionIndex()}
                onKeyDown={(event) => onRowKeyDown(event, { kind: "section", id: section.id })}
              >
                {handle(`Move ${section.name}`, () => ({ kind: "section", id: section.id }))}
                <NameInput
                  value={section.name}
                  label="Section name"
                  class="edit-row__name--section"
                  ref={(el) => nameInputs.set(section.id, el)}
                  onSave={(name) => void store.renameSection(section.id, name)}
                />
                <button
                  type="button"
                  class="icon-btn icon-btn--small edit-row__delete"
                  aria-label={`Delete section ${section.name}`}
                  title="Delete section"
                  onClick={() => deleteSection(section)}
                >
                  <Trash class="icon" aria-hidden="true" />
                </button>
              </div>
              <Show when={confirmSection() === section.id}>
                <div class="edit-row__confirm" role="alert">
                  <span>
                    Delete “{section.name}” and its {section.views.length}{" "}
                    {section.views.length === 1 ? "view" : "views"}?
                  </span>
                  <div class="edit-row__confirm-actions">
                    <button
                      type="button"
                      class="btn btn--small"
                      onClick={() => setConfirmSection(null)}
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      class="btn btn--small btn--danger-solid"
                      onClick={() => deleteSection(section)}
                    >
                      Delete
                    </button>
                  </div>
                </div>
              </Show>
              <For each={section.views}>
                {(view, viewIndex) => (
                  <>
                    <Show
                      when={showsLine({ kind: "view", sectionId: section.id, index: viewIndex() })}
                    >
                      <div class="edit-row__drop" />
                    </Show>
                    <div
                      class="edit-row"
                      classList={{ "edit-row--dragged": dragged()?.id === view.id }}
                      data-edit-row
                      data-kind="view"
                      data-section={section.id}
                      data-index={viewIndex()}
                      onKeyDown={(event) => onRowKeyDown(event, { kind: "view", id: view.id })}
                    >
                      {handle(`Move ${view.name}`, () => ({ kind: "view", id: view.id }))}
                      <button
                        type="button"
                        class="icon-btn icon-btn--small"
                        aria-label={`Change icon of ${view.name}`}
                        title="Change icon"
                        onClick={(event) => {
                          const rect = event.currentTarget.getBoundingClientRect();
                          setIconFor({ view, anchor: { rect, align: "start" } });
                        }}
                      >
                        <ViewIcon name={view.icon} />
                      </button>
                      <NameInput
                        value={view.name}
                        label="View name"
                        ref={(el) => nameInputs.set(view.id, el)}
                        onSave={(name) => void store.renameView(view.id, name)}
                      />
                      <button
                        type="button"
                        class="icon-btn icon-btn--small edit-row__delete"
                        aria-label={`Delete view ${view.name}`}
                        title="Delete view"
                        onClick={() => void store.deleteView(view.id)}
                      >
                        <Trash class="icon" aria-hidden="true" />
                      </button>
                    </div>
                  </>
                )}
              </For>
              <Show
                when={showsLine({
                  kind: "view",
                  sectionId: section.id,
                  index: section.views.length,
                })}
              >
                <div class="edit-row__drop" />
              </Show>
              <Show when={section.views.length === 0}>
                <p class="edit-row__empty">Drag views here</p>
              </Show>
            </>
          )}
        </For>
        <Show when={showsLine({ kind: "section", index: store.sections().length })}>
          <div class="edit-row__drop" />
        </Show>
        <button type="button" class="sidebar-editor__add" onClick={() => void addSection()}>
          <Plus class="icon" aria-hidden="true" /> New section
        </button>
      </div>
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
    </div>
  );
}
