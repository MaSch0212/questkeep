import Plus from "lucide-solid/icons/plus";
import { createEffect, createSignal, For, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import type { Game } from "../bindings/Game";
import type { GameScripts } from "../bindings/GameScripts";
import type { RuntimeInfo } from "../bindings/RuntimeInfo";
import type { Script } from "../bindings/Script";
import { translator } from "../i18n";
import { scriptsText } from "../i18n/scripts";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { duplicateOf } from "../lib/scriptDraft";
import { appliedRows, kindWord, moveBy, moveItem } from "../lib/scripts";
import { store } from "../lib/store";
import { ConfirmDialog } from "./ConfirmDialog";
import { Menu, type MenuEntry } from "./Menu";
import { ScriptDialog } from "./ScriptDialog";
import { type ListRow, ScriptList, ScriptRowContent } from "./ScriptList";

const tr = translator(scriptsText);

const editing = (open: Script | "new"): Script | null => (open === "new" ? null : open);

/**
 * The Scripts tab of the Edit dialog: the game's own scripts (changes save at once: scripts are
 * their own records), the opt-out of global scripts, and what applies to the game in run order.
 */
export function ScriptsTab(props: {
  game: Game;
  scripts: GameScripts | null | undefined;
  /** Loads the scripts again (after a change made here). */
  reload: () => void;
  /** The Edit dialog's "Open in Settings" (it asks first when it has unsaved changes). */
  onOpenSettings: () => void;
}) {
  const [runtimes, setRuntimes] = createSignal<RuntimeInfo[]>([]);
  const [dialog, setDialog] = createSignal<Script | "new" | null>(null);
  const [menu, setMenu] = createSignal<{ script: Script; rect: DOMRect } | null>(null);
  const [deleting, setDeleting] = createSignal<Script | null>(null);
  const [runGlobal, setRunGlobal] = createSignal(props.scripts?.runGlobal ?? true);
  createEffect(() => setRunGlobal(props.scripts?.runGlobal ?? true));

  onMount(() => {
    void Promise.resolve(api.scriptRuntimes(false))
      .then((list) => setRuntimes(list ?? []))
      .catch(() => undefined);
  });

  const own = () => (props.scripts?.own ?? []).toSorted((a, b) => Number(a.position - b.position));
  const rows = (): ListRow[] => own().map((script) => ({ key: script.id, kind: "script", script }));

  async function report(work: Promise<unknown>) {
    try {
      await work;
    } catch (failure) {
      store.pushToast("error", errorMessage(failure));
    }
    props.reload();
  }

  function reorder(next: ListRow[]) {
    const ids = next.flatMap((r) => (r.kind === "script" ? [r.script.id] : []));
    void report(api.setOwnScriptOrder(ids));
  }

  function nudge(key: string, by: -1 | 1) {
    const list = rows();
    const index = list.findIndex((r) => r.key === key);
    if (index >= 0) reorder(moveBy(list, index, by));
  }

  function move(from: string, to: string) {
    const list = rows();
    const a = list.findIndex((r) => r.key === from);
    const b = list.findIndex((r) => r.key === to);
    if (a >= 0 && b >= 0 && a !== b) reorder(moveItem(list, a, b));
  }

  const menuEntries = (script: Script): MenuEntry[] => {
    const list = rows();
    const index = list.findIndex((r) => r.key === script.id);
    return [
      {
        label: tr("duplicate"),
        onSelect: () =>
          void report(
            api.saveScript(duplicateOf(script, tr("duplicateName", { name: script.name }))),
          ),
      },
      {
        label: tr("runThisNow"),
        onSelect: () =>
          void report(
            Promise.resolve(api.runScript(script.id, props.game.id)).then(() =>
              store.pushToast(
                "info",
                tr("runStarted", { name: script.name, game: props.game.name }),
              ),
            ),
          ),
      },
      { kind: "separator" },
      { label: tr("moveUp"), disabled: index <= 0, onSelect: () => nudge(script.id, -1) },
      {
        label: tr("moveDown"),
        disabled: index < 0 || index >= list.length - 1,
        onSelect: () => nudge(script.id, 1),
      },
      { kind: "separator" },
      { label: tr("delete"), danger: true, onSelect: () => setDeleting(script) },
    ];
  };

  const applied = () => appliedRows(props.scripts?.applied ?? []);

  return (
    <div class="scripts-tab">
      <section class="script-card" aria-label={tr("ownTitle")}>
        <header class="script-card__head">
          <h3 class="script-card__title">{tr("ownTitle")}</h3>
          <button type="button" class="btn" onClick={() => setDialog("new")}>
            <Plus class="icon" aria-hidden="true" />
            {tr("addScript")}
          </button>
        </header>
        <Show
          when={rows().length > 0}
          fallback={<p class="muted script-card__empty">{tr("noOwn")}</p>}
        >
          <ScriptList
            label={tr("ownTitle")}
            rows={rows()}
            reorderable={rows().length > 1}
            onMove={move}
            onNudge={nudge}
            row={(row) =>
              row.kind === "script" ? (
                <ScriptRowContent
                  script={row.script}
                  runtimes={runtimes()}
                  own
                  onToggle={(enabled) => void report(api.setScriptEnabled(row.script.id, enabled))}
                  onEdit={() => setDialog(row.script)}
                  onMore={(button) =>
                    setMenu({ script: row.script, rect: button.getBoundingClientRect() })
                  }
                />
              ) : null
            }
          />
        </Show>
      </section>

      <label class="check">
        <input
          type="checkbox"
          checked={runGlobal()}
          onChange={(event) => {
            const run = event.currentTarget.checked;
            setRunGlobal(run);
            void report(api.setRunGlobalScripts(props.game.id, run));
          }}
        />
        {tr("runGlobal")}
      </label>

      <section class="script-card" aria-label={tr("appliesTitle")}>
        <header class="script-card__head">
          <h3 class="script-card__title">{tr("appliesTitle")}</h3>
          <button type="button" class="btn btn--ghost" onClick={() => props.onOpenSettings()}>
            {tr("openSettings")}
          </button>
        </header>
        <Show
          when={applied().length > 0}
          fallback={<p class="muted script-card__empty">{tr("appliesNone")}</p>}
        >
          <ul class="script-applies" aria-label={tr("appliesTitle")}>
            <For each={applied()}>
              {(row) => (
                <li class="script-applies__row">
                  <span class="script-badge">{kindWord(row.kind)}</span>
                  <Show
                    when={row.script}
                    fallback={
                      <>
                        <i class="script-applies__own">{tr("appliesOwn")}</i>
                        <span class="field__hint">{row.own?.map((s) => s.name).join(", ")}</span>
                      </>
                    }
                  >
                    {(script) => (
                      <>
                        <span class="script-applies__name">{script().name}</span>
                        <Show when={script().filter !== null}>
                          <span class="field__hint">· {tr("appliesFilterHint")}</span>
                        </Show>
                      </>
                    )}
                  </Show>
                </li>
              )}
            </For>
          </ul>
        </Show>
        <p class="field__hint">{tr("appliesHint")}</p>
      </section>

      <Show when={menu()}>
        {(open) => (
          <Menu
            anchor={{ rect: open().rect, align: "end" }}
            label={open().script.name}
            entries={menuEntries(open().script)}
            onClose={() => setMenu(null)}
          />
        )}
      </Show>
      <Show when={deleting()}>
        {(script) => (
          <Portal>
            {/* Outside the Edit dialog, so its focus trap doesn't fight this dialog's. */}
            <ConfirmDialog
              title={tr("deleteTitle")}
              message={tr("deleteMessage", { name: script().name })}
              confirmLabel={tr("delete")}
              onConfirm={() => void report(api.deleteScript(script().id))}
              onClose={() => setDeleting(null)}
            />
          </Portal>
        )}
      </Show>
      <Show when={dialog()}>
        {(open) => (
          <Portal>
            <ScriptDialog
              script={editing(open())}
              kind="pre"
              gameId={props.game.id}
              onSaved={() => props.reload()}
              onClose={() => {
                setDialog(null);
                props.reload();
              }}
            />
          </Portal>
        )}
      </Show>
    </div>
  );
}
