import ChevronDown from "lucide-solid/icons/chevron-down";
import ChevronUp from "lucide-solid/icons/chevron-up";
import Trash from "lucide-solid/icons/trash";
import { createSignal, For, onMount, Show } from "solid-js";
import type { CompletionSettings } from "../../bindings/CompletionSettings";
import type { CompletionStatus } from "../../bindings/CompletionStatus";
import { translator } from "../../i18n";
import { settingsLibrary } from "../../i18n/settingsLibrary";
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
      store.pushToast("error", tr("statusExists", { name: trimmed }));
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
      <ol class="source-order status-list" aria-label={tr("statusList")}>
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
                aria-label={tr("statusNameOf", { name: status.name })}
                onChange={(event) => {
                  rename(status, event.currentTarget.value);
                  event.currentTarget.value = status.name;
                }}
              />
              <span class="status-list__count">{tr("games", { count: status.gameCount })}</span>
              <button
                type="button"
                class="icon-btn icon-btn--small"
                aria-label={tr("moveUp", { name: status.name })}
                title={tr("moveUpTitle")}
                disabled={index() === 0}
                onClick={() => move(status.id, -1)}
              >
                <ChevronUp class="icon" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="icon-btn icon-btn--small"
                aria-label={tr("moveDown", { name: status.name })}
                title={tr("moveDownTitle")}
                disabled={index() === statuses().length - 1}
                onClick={() => move(status.id, 1)}
              >
                <ChevronDown class="icon" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="icon-btn icon-btn--small"
                aria-label={tr("removeStatus", { name: status.name })}
                title={tr("removeTitle")}
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
