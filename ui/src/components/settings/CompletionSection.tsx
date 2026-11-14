import GripVertical from "lucide-solid/icons/grip-vertical";
import Pencil from "lucide-solid/icons/pencil";
import Trash from "lucide-solid/icons/trash";
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import type { CompletionSettings } from "../../bindings/CompletionSettings";
import type { CompletionStatus } from "../../bindings/CompletionStatus";
import { translator } from "../../i18n";
import { settingsLibrary } from "../../i18n/settingsLibrary";
import { dropIndexAt, moveBefore, nudge } from "../../lib/categoryOrder";
import { store } from "../../lib/store";

const tr = translator(settingsLibrary);

const sameName = (a: string, b: string) =>
  a.trim().localeCompare(b.trim(), undefined, { sensitivity: "base" }) === 0;

/** "none" in the selects stands for no status. */
const NONE = "";

const settings = () => store.completion();
const statuses = () => settings().statuses;

const save = (next: CompletionSettings, replacements: Record<string, string | null> = {}) =>
  store.saveCompletion(next, replacements);

function withStatuses(list: CompletionStatus[]): CompletionSettings {
  return { ...settings(), statuses: list };
}

const options = (except?: string) => statuses().filter((s) => s.id !== except);

/**
 * The user's completion statuses: add, rename, reorder (drag a row by its handle, or Alt+Up/Down)
 * and remove
 * (games with a removed status get another one), plus the status of new games and the one set
 * when a game is started for the first time. Every change is saved at once.
 */
export function CompletionSection() {
  const [adding, setAdding] = createSignal("");
  const [removing, setRemoving] = createSignal<CompletionStatus | null>(null);
  const [replacement, setReplacement] = createSignal(NONE);
  const [dragged, setDragged] = createSignal<string | null>(null);
  const [drop, setDrop] = createSignal<number | null>(null);
  let list: HTMLOListElement | undefined;
  // Fresh game counts.
  onMount(() => void store.loadCompletion());

  function rename(status: CompletionStatus, name: string) {
    const trimmed = name.trim();
    if (trimmed === "" || trimmed === status.name) return;
    if (statuses().some((s) => s.id !== status.id && sameName(s.name, trimmed))) {
      store.pushToast("error", tr("statusExists", { name: trimmed }));
      return;
    }
    const index = statuses().findIndex((s) => s.id === status.id);
    void save(withStatuses(statuses().with(index, { ...status, name: trimmed })));
  }

  const ids = () => statuses().map((s) => s.id);

  /** Saves the statuses in the order of `next`; `refocus` puts focus back on the moved row. */
  function reorder(next: string[], moved: string, refocus: boolean) {
    if (next.join() === ids().join()) return;
    const byId = new Map(statuses().map((s) => [s.id, s]));
    void save(withStatuses(next.flatMap((id) => byId.get(id) ?? []))).then(() => {
      if (refocus) list?.querySelector<HTMLElement>(`[data-status="${moved}"]`)?.focus();
    });
  }

  function startDrag(event: PointerEvent, id: string) {
    if (event.button !== 0) return;
    event.preventDefault();
    setDragged(id);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp, { once: true });
  }

  function onPointerMove(event: PointerEvent) {
    if (!list || !dragged()) return;
    const middles = [...list.querySelectorAll<HTMLElement>("[data-status]")].map((row) => {
      const rect = row.getBoundingClientRect();
      return rect.top + rect.height / 2;
    });
    setDrop(dropIndexAt(middles, event.clientY));
  }

  function onPointerUp() {
    window.removeEventListener("pointermove", onPointerMove);
    const id = dragged();
    const index = drop();
    setDragged(null);
    setDrop(null);
    if (id && index !== null) reorder(moveBefore(ids(), id, index), id, false);
  }

  onCleanup(() => {
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
  });

  function onRowKeyDown(event: KeyboardEvent, id: string) {
    if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
    if (event.target instanceof HTMLInputElement) return;
    event.preventDefault();
    reorder(nudge(ids(), id, event.key === "ArrowUp" ? -1 : 1), id, true);
  }

  async function add(event: SubmitEvent) {
    event.preventDefault();
    const name = adding().trim();
    if (name === "") return;
    if (statuses().some((s) => sameName(s.name, name))) {
      store.pushToast("error", tr("statusExists", { name }));
      return;
    }
    if (await save(withStatuses([...statuses(), { id: "", name, gameCount: 0 }]))) setAdding("");
  }

  function remove(status: CompletionStatus, to: string | null) {
    setRemoving(null);
    void save(withStatuses(statuses().filter((s) => s.id !== status.id)), { [status.id]: to });
  }

  function askRemove(status: CompletionStatus) {
    if (status.gameCount === 0) {
      remove(status, null);
      return;
    }
    setReplacement(statuses().find((s) => s.id !== status.id)?.id ?? NONE);
    setRemoving(status);
  }

  return (
    <div class="form">
      <p class="muted">{tr("completionIntro")}</p>
      <ol
        class="category-list"
        classList={{ "category-list--dragging": dragged() !== null }}
        aria-label={tr("statusList")}
        ref={(el) => (list = el)}
      >
        <For each={statuses()}>
          {(status, index) => (
            <>
              <Show when={drop() === index() && dragged() !== null}>
                <li class="category-list__drop" aria-hidden="true" />
              </Show>
              <StatusRow
                status={status}
                dragged={dragged() === status.id}
                onDragStart={(event) => startDrag(event, status.id)}
                onKeyDown={(event) => onRowKeyDown(event, status.id)}
                onRename={(name) => rename(status, name)}
                onRemove={() => askRemove(status)}
              />
            </>
          )}
        </For>
        <Show when={drop() === statuses().length && dragged() !== null}>
          <li class="category-list__drop" aria-hidden="true" />
        </Show>
      </ol>
      <Show when={removing()}>
        {(status) => (
          <div
            class="status-remove"
            role="group"
            aria-label={tr("removeStatus", { name: status().name })}
          >
            <span>{tr("removePrompt", { count: status().gameCount, name: status().name })}</span>
            <select
              class="input"
              aria-label={tr("statusForGames")}
              value={replacement()}
              onChange={(event) => setReplacement(event.currentTarget.value)}
            >
              <For each={options(status().id)}>{(s) => <option value={s.id}>{s.name}</option>}</For>
              <option value={NONE}>{tr("noStatus")}</option>
            </select>
            <button
              type="button"
              class="btn btn--danger"
              onClick={() => remove(status(), replacement() === NONE ? null : replacement())}
            >
              {tr("removeButton")}
            </button>
            <button type="button" class="btn btn--ghost" onClick={() => setRemoving(null)}>
              {tr("cancel")}
            </button>
          </div>
        )}
      </Show>
      <form class="field__row status-add" onSubmit={(event) => void add(event)}>
        <input
          class="input"
          placeholder={tr("newPlaceholder")}
          aria-label={tr("newAria")}
          value={adding()}
          onInput={(event) => setAdding(event.currentTarget.value)}
        />
        <button type="submit" class="btn" disabled={adding().trim() === ""}>
          {tr("addStatus")}
        </button>
      </form>
      <div class="field">
        <label class="field__label" for="completion-new-game">
          {tr("newGamesGet")}
        </label>
        <select
          id="completion-new-game"
          class="input status-select"
          value={settings().newGame ?? NONE}
          onChange={(event) =>
            void save({
              ...settings(),
              newGame: event.currentTarget.value === NONE ? null : event.currentTarget.value,
            })
          }
        >
          <For each={statuses()}>{(s) => <option value={s.id}>{s.name}</option>}</For>
          <option value={NONE}>{tr("noStatus")}</option>
        </select>
      </div>
      <div class="field">
        <label class="field__label" for="completion-first-start">
          {tr("firstStart")}
        </label>
        <select
          id="completion-first-start"
          class="input status-select"
          value={settings().firstStart ?? NONE}
          onChange={(event) =>
            void save({
              ...settings(),
              firstStart: event.currentTarget.value === NONE ? null : event.currentTarget.value,
            })
          }
        >
          <For each={statuses()}>
            {(s) => <option value={s.id}>{tr("setItTo", { name: s.name })}</option>}
          </For>
          <option value={NONE}>{tr("leaveAlone")}</option>
        </select>
        <p class="field__hint">{tr("firstStartHint")}</p>
      </div>
    </div>
  );
}

/** One status: drag handle, name (an input while renaming), game count, rename and remove. */
function StatusRow(props: {
  status: CompletionStatus;
  dragged: boolean;
  onDragStart: (event: PointerEvent) => void;
  onKeyDown: (event: KeyboardEvent) => void;
  onRename: (name: string) => void;
  onRemove: () => void;
}) {
  const [editing, setEditing] = createSignal(false);
  let renameButton: HTMLButtonElement | undefined;

  /** Ends renaming; `null` cancels. */
  function finish(value: string | null) {
    setEditing(false);
    if (value !== null) props.onRename(value);
    queueMicrotask(() => renameButton?.focus());
  }

  return (
    <li
      class="category-list__row"
      classList={{ "category-list__row--dragged": props.dragged }}
      data-status={props.status.id}
      tabIndex={-1}
      onKeyDown={(event) => props.onKeyDown(event)}
    >
      <span
        class="category-list__handle"
        data-tooltip={tr("dragTitle")}
        aria-hidden="true"
        onPointerDown={(event) => props.onDragStart(event)}
      >
        <GripVertical class="icon" />
      </span>
      <Show
        when={editing()}
        fallback={<span class="category-list__name">{props.status.name}</span>}
      >
        <input
          ref={(el) => queueMicrotask(() => el.select())}
          class="input category-list__input"
          aria-label={tr("statusNameOf", { name: props.status.name })}
          value={props.status.name}
          onBlur={(event) => {
            if (editing()) finish(event.currentTarget.value);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              finish(event.currentTarget.value);
            } else if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              finish(null);
            }
          }}
        />
      </Show>
      <span class="category-list__count">{tr("games", { count: props.status.gameCount })}</span>
      <button
        ref={(el) => (renameButton = el)}
        type="button"
        class="icon-btn icon-btn--small"
        aria-label={tr("renameStatus", { name: props.status.name })}
        data-tooltip={tr("renameTitle")}
        onClick={() => setEditing(true)}
      >
        <Pencil class="icon" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="icon-btn icon-btn--small category-list__delete"
        aria-label={tr("removeStatus", { name: props.status.name })}
        data-tooltip={tr("removeTitle")}
        onClick={() => props.onRemove()}
      >
        <Trash class="icon" aria-hidden="true" />
      </button>
    </li>
  );
}
