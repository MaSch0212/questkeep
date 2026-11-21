import GripVertical from "lucide-solid/icons/grip-vertical";
import X from "lucide-solid/icons/x";
import { createResource, createSignal, For, Show } from "solid-js";
import type { Game } from "../../bindings/Game";
import type { QuickLaunchItem } from "../../bindings/QuickLaunchItem";
import type { TrayIconStyle } from "../../bindings/TrayIconStyle";
import { translator } from "../../i18n";
import { settingsGeneral } from "../../i18n/settingsGeneral";
import * as api from "../../lib/api";
import { moveItem } from "../../lib/editGame";
import { defaultQuery } from "../../lib/query";
import {
  moveQuickItemTo,
  parseQuickKey,
  quickKey,
  quickLaunchItems,
  withQuickItem,
} from "../../lib/quickLaunch";
import { store } from "../../lib/store";
import { GameIcon } from "../GameIcon";
import { Modal } from "../Modal";
import { SourceIcon, sourceName } from "../SourceIcon";
import { ControllerGlyph, QuestkeepGlyph } from "./TrayGlyphs";
import type { SettingsModel } from "./useSettings";

const tr = translator(settingsGeneral);

const ICONS: readonly [
  TrayIconStyle,
  "trayIconQuestkeep" | "trayIconController",
  typeof QuestkeepGlyph,
][] = [
  ["questkeep", "trayIconQuestkeep", QuestkeepGlyph],
  ["controller", "trayIconController", ControllerGlyph],
];

/** Apps of the library, by name: what can be added to the quick-launch strip. */
const loadApps = () => api.listGames({ ...defaultQuery(), kind: "apps" });

/**
 * The tray: its icon, and the quick-launch strip of the tray popup (store launchers and apps,
 * in the user's order) and how many rows it has. Entries move by dragging, with their buttons or
 * Alt+Up/Down.
 */
export function TraySettings(props: { model: SettingsModel }) {
  const [apps] = createResource(loadApps);
  const [adding, setAdding] = createSignal("");
  const [editing, setEditing] = createSignal(false);
  /** Key of the entry being dragged, and the order while dragging (saved when it is dropped). */
  const [dragged, setDragged] = createSignal<string | null>(null);
  const [draft, setDraft] = createSignal<QuickLaunchItem[] | null>(null);
  let list: HTMLUListElement | undefined;

  const tray = () =>
    props.model.settings()?.tray ?? { icon: "questkeep", quickLaunchRows: 3, quickLaunch: null };
  const items = () => draft() ?? quickLaunchItems(tray().quickLaunch, store.launchers());
  const app = (id: string): Game | undefined => apps()?.find((a) => a.id === id);
  const label = (item: QuickLaunchItem) =>
    item.kind === "store" ? sourceName(item.id) : (app(item.id)?.name ?? tr("quickLaunchGone"));

  /** What the add list offers: installed launchers and apps not in the strip yet. */
  const addable = () => {
    const taken = new Set(items().map(quickKey));
    const stores: QuickLaunchItem[] = store.launchers().map((id) => ({ kind: "store", id }));
    const games: QuickLaunchItem[] = (apps() ?? []).map((a) => ({ kind: "game", id: a.id }));
    return {
      stores: stores.filter((i) => !taken.has(quickKey(i))),
      games: games.filter((i) => !taken.has(quickKey(i))),
    };
  };

  async function save(next: QuickLaunchItem[] | null, focusKey?: string) {
    await props.model.update({ tray: { ...tray(), quickLaunch: next } });
    if (focusKey) {
      list?.querySelector<HTMLElement>(`[data-quick-key="${CSS.escape(focusKey)}"]`)?.focus();
    }
  }

  async function drop() {
    const next = draft();
    setDragged(null);
    if (next) await save(next);
    setDraft(null);
  }

  function move(index: number, by: -1 | 1) {
    const current = items();
    const key = current[index] ? quickKey(current[index]) : undefined;
    void save(moveItem(current, index, by), key);
  }

  function add() {
    const item = parseQuickKey(adding());
    if (!item) return;
    setAdding("");
    void save(withQuickItem(items(), item));
  }

  return (
    <>
      <div class="settings-row">
        <span class="settings-row__text" id="tray-icon-label">
          {tr("trayIcon")}
        </span>
        <div class="segmented" role="radiogroup" aria-labelledby="tray-icon-label">
          <For each={ICONS}>
            {([value, text, Glyph]) => (
              <button
                type="button"
                role="radio"
                class="segmented__option segmented__option--icon"
                aria-checked={tray().icon === value}
                aria-label={tr(text)}
                data-tooltip={tr(text)}
                onClick={() => void props.model.update({ tray: { ...tray(), icon: value } })}
              >
                <Glyph class="icon" />
              </button>
            )}
          </For>
        </div>
      </div>
      <div class="settings-row">
        <div class="settings-row__text">
          <span id="quick-launch-rows-label">{tr("quickLaunchRows")}</span>
          <span class="field__hint" id="quick-launch-rows-hint">
            {tr("quickLaunchRowsHint")}
          </span>
        </div>
        <div
          class="segmented"
          role="radiogroup"
          aria-labelledby="quick-launch-rows-label"
          aria-describedby="quick-launch-rows-hint"
        >
          <For each={[1, 2, 3]}>
            {(n) => (
              <button
                type="button"
                role="radio"
                class="segmented__option"
                aria-checked={tray().quickLaunchRows === n}
                onClick={() => void props.model.update({ tray: { ...tray(), quickLaunchRows: n } })}
              >
                {n}
              </button>
            )}
          </For>
        </div>
      </div>
      <div class="settings-row">
        <span class="settings-row__label">
          {tr("quickLaunch")}
          <span class="field__hint"> · {tr("quickLaunchCount", { count: items().length })}</span>
        </span>
        <button type="button" class="btn" onClick={() => setEditing(true)}>
          {tr("quickLaunchEdit")}
        </button>
      </div>
      <Show when={editing()}>
        <Modal title={tr("quickLaunch")} onClose={() => setEditing(false)}>
          <p class="field__hint" id="quick-launch-hint">
            {tr("quickLaunchHint")}
          </p>
          <Show when={items().length > 0} fallback={<p class="muted">{tr("quickLaunchEmpty")}</p>}>
            <ul class="quick-launch" ref={(el) => (list = el)} aria-describedby="quick-launch-hint">
              <For each={items()}>
                {(item, index) => (
                  <li
                    class="quick-launch__item"
                    classList={{ "quick-launch__item--dragging": dragged() === quickKey(item) }}
                    data-quick-key={quickKey(item)}
                    tabIndex={0}
                    draggable={true}
                    onDragStart={(event) => {
                      setDragged(quickKey(item));
                      event.dataTransfer?.setData("text/plain", quickKey(item));
                    }}
                    onDragEnd={() => void drop()}
                    onDragOver={(event) => {
                      const from = dragged();
                      if (from === null) return;
                      event.preventDefault();
                      // The list follows the pointer, so the drop needs no target marker.
                      if (from !== quickKey(item)) {
                        setDraft(moveQuickItemTo(items(), from, quickKey(item)));
                      }
                    }}
                    onDrop={(event) => event.preventDefault()}
                    onKeyDown={(event) => {
                      if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) {
                        return;
                      }
                      event.preventDefault();
                      move(index(), event.key === "ArrowUp" ? -1 : 1);
                    }}
                  >
                    <GripVertical class="icon quick-launch__grip" aria-hidden="true" />
                    <span class="quick-launch__icon" aria-hidden="true">
                      <Show
                        when={item.kind === "game" && app(item.id)}
                        fallback={<SourceIcon source={item.id} />}
                      >
                        {(game) => <GameIcon game={game()} />}
                      </Show>
                    </span>
                    <span class="quick-launch__name">{label(item)}</span>
                    <button
                      type="button"
                      class="icon-btn icon-btn--small"
                      aria-label={tr("removeFromQuickLaunch", { name: label(item) })}
                      data-tooltip={tr("removeFromQuickLaunch", { name: label(item) })}
                      onClick={() =>
                        void save(items().filter((i) => quickKey(i) !== quickKey(item)))
                      }
                    >
                      <X class="icon" aria-hidden="true" />
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
          <div class="settings-row">
            <label for="quick-launch-add">{tr("quickLaunchAdd")}</label>
            <div class="quick-launch__add">
              <select
                id="quick-launch-add"
                class="input settings-row__input"
                value={adding()}
                onChange={(e) => setAdding(e.currentTarget.value)}
              >
                <option value="">{tr("quickLaunchPick")}</option>
                <Show when={addable().stores.length > 0}>
                  <optgroup label={tr("quickLaunchStores")}>
                    <For each={addable().stores}>
                      {(item) => <option value={quickKey(item)}>{label(item)}</option>}
                    </For>
                  </optgroup>
                </Show>
                <Show when={addable().games.length > 0}>
                  <optgroup label={tr("quickLaunchApps")}>
                    <For each={addable().games}>
                      {(item) => <option value={quickKey(item)}>{label(item)}</option>}
                    </For>
                  </optgroup>
                </Show>
              </select>
              <button type="button" class="btn" disabled={adding() === ""} onClick={add}>
                {tr("quickLaunchAddButton")}
              </button>
            </div>
          </div>
          <Show when={tray().quickLaunch !== null}>
            <button type="button" class="link-btn" onClick={() => void save(null)}>
              {tr("quickLaunchReset")}
            </button>
          </Show>
        </Modal>
      </Show>
    </>
  );
}
