import ChevronDown from "lucide-solid/icons/chevron-down";
import ChevronUp from "lucide-solid/icons/chevron-up";
import Trash from "lucide-solid/icons/trash";
import { createSignal, For, onMount, Show } from "solid-js";
import type { CompletionSettings } from "../../bindings/CompletionSettings";
import type { CompletionStatus } from "../../bindings/CompletionStatus";
import { store } from "../../lib/store";

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
const noun = (n: number) => (n === 1 ? "game" : "games");

/**
 * The user's completion statuses: add, rename, reorder (arrow buttons or Alt+Up/Down) and remove
 * (games with a removed status get another one), plus the status of new games and the one set
 * when a game is started for the first time. Every change is saved at once.
 */
export function CompletionSection() {
  const [adding, setAdding] = createSignal("");
  const [removing, setRemoving] = createSignal<CompletionStatus | null>(null);
  const [replacement, setReplacement] = createSignal(NONE);
  // Fresh game counts.
  onMount(() => void store.loadCompletion());

  function rename(status: CompletionStatus, name: string) {
    const trimmed = name.trim();
    if (trimmed === "" || trimmed === status.name) return;
    if (statuses().some((s) => s.id !== status.id && sameName(s.name, trimmed))) {
      store.pushToast("error", `There already is a status named “${trimmed}”.`);
      return;
    }
    const index = statuses().findIndex((s) => s.id === status.id);
    void save(withStatuses(statuses().with(index, { ...status, name: trimmed })));
  }

  function move(id: string, by: number) {
    const list = [...statuses()];
    const from = list.findIndex((s) => s.id === id);
    const to = from + by;
    const status = list[from];
    if (!status || to < 0 || to >= list.length) return;
    list.splice(from, 1);
    list.splice(to, 0, status);
    void save(withStatuses(list)).then(() =>
      document.querySelector<HTMLElement>(`[data-status="${id}"]`)?.focus(),
    );
  }

  async function add(event: SubmitEvent) {
    event.preventDefault();
    const name = adding().trim();
    if (name === "") return;
    if (statuses().some((s) => sameName(s.name, name))) {
      store.pushToast("error", `There already is a status named “${name}”.`);
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
      <p class="muted">
        Track how far you got with each game. Sort and group views by status, or filter by it. Move
        a status with its arrows or Alt+Up/Down; the order is also the sort order.
      </p>
      <ol class="source-order status-list" aria-label="Completion statuses">
        <For each={statuses()}>
          {(status, index) => (
            <li
              class="source-order__row"
              data-status={status.id}
              tabIndex={-1}
              onKeyDown={(event) => {
                if (!event.altKey) return;
                if (event.key === "ArrowUp") move(status.id, -1);
                else if (event.key === "ArrowDown") move(status.id, 1);
                else return;
                event.preventDefault();
              }}
            >
              <input
                class="input status-list__name"
                value={status.name}
                aria-label={`Name of ${status.name}`}
                onChange={(event) => {
                  rename(status, event.currentTarget.value);
                  event.currentTarget.value = status.name;
                }}
              />
              <span class="status-list__count">
                {status.gameCount} {noun(status.gameCount)}
              </span>
              <button
                type="button"
                class="icon-btn icon-btn--small"
                aria-label={`Move ${status.name} up`}
                title="Move up (Alt+Up)"
                disabled={index() === 0}
                onClick={() => move(status.id, -1)}
              >
                <ChevronUp class="icon" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="icon-btn icon-btn--small"
                aria-label={`Move ${status.name} down`}
                title="Move down (Alt+Down)"
                disabled={index() === statuses().length - 1}
                onClick={() => move(status.id, 1)}
              >
                <ChevronDown class="icon" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="icon-btn icon-btn--small"
                aria-label={`Remove ${status.name}`}
                title="Remove"
                onClick={() => askRemove(status)}
              >
                <Trash class="icon" aria-hidden="true" />
              </button>
            </li>
          )}
        </For>
      </ol>
      <Show when={removing()}>
        {(status) => (
          <div class="status-remove" role="group" aria-label={`Remove ${status().name}`}>
            <span>
              {status().gameCount} {noun(status().gameCount)}{" "}
              {status().gameCount === 1 ? "has" : "have"} “{status().name}”. Give them:
            </span>
            <select
              class="input"
              aria-label="Status for those games"
              value={replacement()}
              onChange={(event) => setReplacement(event.currentTarget.value)}
            >
              <For each={options(status().id)}>{(s) => <option value={s.id}>{s.name}</option>}</For>
              <option value={NONE}>No status</option>
            </select>
            <button
              type="button"
              class="btn btn--danger"
              onClick={() => remove(status(), replacement() === NONE ? null : replacement())}
            >
              Remove
            </button>
            <button type="button" class="btn btn--ghost" onClick={() => setRemoving(null)}>
              Cancel
            </button>
          </div>
        )}
      </Show>
      <form class="field__row status-add" onSubmit={(event) => void add(event)}>
        <input
          class="input"
          placeholder="New status, e.g. Abandoned"
          aria-label="New status name"
          value={adding()}
          onInput={(event) => setAdding(event.currentTarget.value)}
        />
        <button type="submit" class="btn" disabled={adding().trim() === ""}>
          Add status
        </button>
      </form>
      <div class="field">
        <label class="field__label" for="completion-new-game">
          New games get
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
          <option value={NONE}>No status</option>
        </select>
      </div>
      <div class="field">
        <label class="field__label" for="completion-first-start">
          When a game is started for the first time
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
          <For each={statuses()}>{(s) => <option value={s.id}>Set it to {s.name}</option>}</For>
          <option value={NONE}>Leave the status alone</option>
        </select>
        <p class="field__hint">Only changes games that still have the status new games get.</p>
      </div>
    </div>
  );
}
