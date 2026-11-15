import Plus from "lucide-solid/icons/plus";
import { createEffect, createSignal, For, on, onMount, Show } from "solid-js";
import type { Game } from "../../bindings/Game";
import type { GlobalScripts } from "../../bindings/GlobalScripts";
import type { RuntimeInfo } from "../../bindings/RuntimeInfo";
import type { Script } from "../../bindings/Script";
import type { ScriptKind } from "../../bindings/ScriptKind";
import type { ScriptSlot } from "../../bindings/ScriptSlot";
import { translator } from "../../i18n";
import { scriptsText } from "../../i18n/scripts";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { duplicateOf } from "../../lib/scriptDraft";
import { moveBy, moveItem } from "../../lib/scripts";
import { store } from "../../lib/store";
import { ConfirmDialog } from "../ConfirmDialog";
import { GamePicker } from "../GamePicker";
import { Menu, type MenuEntry } from "../Menu";
import type { PopoverAnchor } from "../Popover";
import { ScriptDialog } from "../ScriptDialog";
import { type ListRow, ScriptList, ScriptRowContent } from "../ScriptList";

const tr = translator(scriptsText);

/** The hint of the games' own scripts row: how many games have some of that kind. */
const ownHint = (count: number) => (count > 0 ? tr("ownHintCount", { count }) : tr("ownHint"));

const CARDS: readonly {
  kind: ScriptKind;
  title: "preTitle" | "postTitle" | "actionsTitle";
  add: "addPre" | "addPost" | "addAction";
}[] = [
  { kind: "pre", title: "preTitle", add: "addPre" },
  { kind: "post", title: "postTitle", add: "addPost" },
  { kind: "action", title: "actionsTitle", add: "addAction" },
];

const scriptRow = (script: Script): ListRow => ({ key: script.id, kind: "script", script });

/** The slot of a row, as the order commands take it. */
const slotOf = (row: ListRow): ScriptSlot =>
  row.kind === "placeholder" ? { type: "gameScripts" } : { type: "script", id: row.script.id };

/**
 * Settings › Scripts: the scripts that run before a game starts, after it closes, and as actions
 * on a game's page. Pre and post are ordered lists with a fixed place for games' own scripts.
 */
export function ScriptsSection() {
  const [global, setGlobal] = createSignal<GlobalScripts | null>(null);
  const [runtimes, setRuntimes] = createSignal<RuntimeInfo[]>([]);
  const [dialog, setDialog] = createSignal<{ script: Script | null; kind: ScriptKind } | null>(
    null,
  );
  const [menu, setMenu] = createSignal<{ script: Script; rect: DOMRect } | null>(null);
  const [runFor, setRunFor] = createSignal<{ script: Script; anchor: PopoverAnchor } | null>(null);
  const [deleting, setDeleting] = createSignal<Script | null>(null);

  async function load() {
    try {
      setGlobal(await api.globalScripts());
    } catch (failure) {
      store.pushToast("error", errorMessage(failure));
    }
  }

  onMount(() => {
    void Promise.resolve(api.scriptRuntimes(false))
      .then((list) => setRuntimes(list ?? []))
      .catch(() => undefined);
  });
  // Loads once, and again after any change of scripts (also by the Edit dialog or the bell).
  createEffect(on(store.scriptsRevision, () => void load()));

  const rows = (kind: ScriptKind): ListRow[] => {
    const all = global();
    if (!all) return [];
    const scripts = all.scripts.filter((s) => s.kind === kind && s.gameId === null);
    if (kind === "action") {
      return scripts.toSorted((a, b) => Number(a.position - b.position)).map(scriptRow);
    }
    const order = kind === "pre" ? all.preOrder : all.postOrder;
    return order.flatMap((slot): ListRow[] => {
      if (slot.type === "gameScripts") {
        return [
          {
            key: "gameScripts",
            kind: "placeholder",
            title: kind === "pre" ? tr("ownPre") : tr("ownPost"),
            hint: ownHint(kind === "pre" ? all.gamesWithPreScripts : all.gamesWithPostScripts),
          },
        ];
      }
      const script = scripts.find((s) => s.id === slot.id);
      return script ? [scriptRow(script)] : [];
    });
  };

  /** Shows a new order at once and saves it; a failure restores what the core has. */
  function reorder(kind: ScriptKind, next: ListRow[]) {
    const slots = next.map(slotOf);
    setGlobal((all) => {
      if (!all) return all;
      if (kind === "pre") return { ...all, preOrder: slots };
      if (kind === "post") return { ...all, postOrder: slots };
      const ids = next.flatMap((r) => (r.kind === "script" ? [r.script.id] : []));
      return {
        ...all,
        scripts: all.scripts.map((s) =>
          s.kind === "action" ? { ...s, position: BigInt(ids.indexOf(s.id)) } : s,
        ),
      };
    });
    void Promise.resolve(api.setScriptOrder(kind, slots)).catch((failure: unknown) => {
      store.pushToast("error", errorMessage(failure));
      void load();
    });
  }

  function move(kind: ScriptKind, from: string, to: string) {
    const list = rows(kind);
    const a = list.findIndex((r) => r.key === from);
    const b = list.findIndex((r) => r.key === to);
    if (a >= 0 && b >= 0 && a !== b) reorder(kind, moveItem(list, a, b));
  }

  function nudge(kind: ScriptKind, key: string, by: -1 | 1) {
    const list = rows(kind);
    const index = list.findIndex((r) => r.key === key);
    if (index >= 0) reorder(kind, moveBy(list, index, by));
  }

  async function report(work: Promise<unknown>) {
    try {
      await work;
    } catch (failure) {
      store.pushToast("error", errorMessage(failure));
      void load();
    }
  }

  async function runNow(script: Script, game: Game) {
    try {
      await api.runScript(script.id, game.id);
      store.pushToast("info", tr("runStarted", { name: script.name, game: game.name }));
    } catch (failure) {
      store.pushToast("error", errorMessage(failure));
    }
  }

  const menuEntries = (script: Script, rect: DOMRect): MenuEntry[] => {
    const list = rows(script.kind);
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
        label: tr("runNowFor"),
        onSelect: () => setRunFor({ script, anchor: { rect, align: "end" } }),
      },
      { kind: "separator" },
      {
        label: tr("moveUp"),
        disabled: index <= 0,
        onSelect: () => nudge(script.kind, script.id, -1),
      },
      {
        label: tr("moveDown"),
        disabled: index < 0 || index >= list.length - 1,
        onSelect: () => nudge(script.kind, script.id, 1),
      },
      { kind: "separator" },
      { label: tr("delete"), danger: true, onSelect: () => setDeleting(script) },
    ];
  };

  return (
    <div class="form scripts-section">
      <p class="muted">{tr("intro")}</p>
      <For each={CARDS}>
        {(card) => (
          <section class="script-card" aria-label={tr(card.title)}>
            <header class="script-card__head">
              <h3 class="script-card__title">{tr(card.title)}</h3>
              <button
                type="button"
                class="btn"
                onClick={() => setDialog({ script: null, kind: card.kind })}
              >
                <Plus class="icon" aria-hidden="true" />
                {tr(card.add)}
              </button>
            </header>
            <Show
              when={rows(card.kind).length > 0}
              fallback={<p class="muted script-card__empty">{tr("empty")}</p>}
            >
              <ScriptList
                label={tr(card.title)}
                rows={rows(card.kind)}
                reorderable={rows(card.kind).length > 1}
                onMove={(from, to) => move(card.kind, from, to)}
                onNudge={(key, by) => nudge(card.kind, key, by)}
                row={(row) =>
                  row.kind === "placeholder" ? (
                    <div class="script-row__main">
                      <b class="script-row__name">{row.title}</b>
                      <small class="script-row__summary">{row.hint}</small>
                    </div>
                  ) : (
                    <ScriptRowContent
                      script={row.script}
                      runtimes={runtimes()}
                      onToggle={(enabled) =>
                        void report(api.setScriptEnabled(row.script.id, enabled))
                      }
                      onEdit={() => setDialog({ script: row.script, kind: row.script.kind })}
                      onMore={(button) =>
                        setMenu({ script: row.script, rect: button.getBoundingClientRect() })
                      }
                    />
                  )
                }
              />
            </Show>
          </section>
        )}
      </For>

      <Show when={menu()}>
        {(open) => (
          <Menu
            anchor={{ rect: open().rect, align: "end" }}
            label={open().script.name}
            entries={menuEntries(open().script, open().rect)}
            onClose={() => setMenu(null)}
          />
        )}
      </Show>
      <Show when={runFor()}>
        {(open) => (
          <GamePicker
            anchor={open().anchor}
            onPick={(game) => void runNow(open().script, game)}
            onClose={() => setRunFor(null)}
          />
        )}
      </Show>
      <Show when={deleting()}>
        {(script) => (
          <ConfirmDialog
            title={tr("deleteTitle")}
            message={tr("deleteMessage", { name: script().name })}
            confirmLabel={tr("delete")}
            onConfirm={() => void report(api.deleteScript(script().id))}
            onClose={() => setDeleting(null)}
          />
        )}
      </Show>
      <Show when={dialog()}>
        {(open) => (
          <ScriptDialog script={open().script} kind={open().kind} onClose={() => setDialog(null)} />
        )}
      </Show>
    </div>
  );
}
