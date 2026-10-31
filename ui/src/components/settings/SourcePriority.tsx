import ChevronDown from "lucide-solid/icons/chevron-down";
import ChevronUp from "lucide-solid/icons/chevron-up";
import GripVertical from "lucide-solid/icons/grip-vertical";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import { createResource, createSignal, For, onCleanup, Show } from "solid-js";
import type { MetadataField } from "../../bindings/MetadataField";
import type { MetadataPriority } from "../../bindings/MetadataPriority";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import {
  effectiveOrder,
  hasOwnOrder,
  moveSource,
  PRIORITY_FIELDS,
  withFieldOrder,
  withGlobalOrder,
} from "../../lib/metadataPriority";
import { store } from "../../lib/store";
import type { SettingsModel } from "./useSettings";

/** Saving re-resolves every game, so moves are saved once they stop for a moment. */
const SAVE_DELAY_MS = 700;

/** An ordered list of sources, moved by dragging, with the arrow buttons or Alt+Up/Down. */
function OrderList(props: {
  label: string;
  order: string[];
  name: (id: string) => string;
  onChange: (order: string[]) => void;
}) {
  const [dragged, setDragged] = createSignal<string | null>(null);
  let list: HTMLOListElement | undefined;
  function move(id: string, by: number) {
    props.onChange(moveSource(props.order, id, by));
    // The row moved; keep the focus on the button or row that moved it.
    queueMicrotask(() =>
      list?.querySelector<HTMLElement>(`[data-source="${id}"]`)?.focus({ preventScroll: false }),
    );
  }
  return (
    <ol ref={(el) => (list = el)} class="source-order" aria-label={props.label}>
      <For each={props.order}>
        {(id, index) => (
          <li
            class="source-order__row"
            classList={{ "source-order__row--dragging": dragged() === id }}
            data-source={id}
            tabIndex={0}
            draggable={true}
            onDragStart={(event) => {
              setDragged(id);
              event.dataTransfer?.setData("text/plain", id);
            }}
            onDragEnd={() => setDragged(null)}
            onDragOver={(event) => {
              const from = dragged();
              if (from === null) return;
              event.preventDefault();
              // The list follows the pointer, so the drop needs no target marker.
              if (from !== id) {
                props.onChange(moveSource(props.order, from, index() - props.order.indexOf(from)));
              }
            }}
            onDrop={(event) => {
              event.preventDefault();
              setDragged(null);
            }}
            onKeyDown={(event) => {
              if (!event.altKey) return;
              if (event.key === "ArrowUp") move(id, -1);
              else if (event.key === "ArrowDown") move(id, 1);
              else return;
              event.preventDefault();
            }}
          >
            <GripVertical class="icon source-order__grip" aria-hidden="true" />
            <span class="source-order__rank">{index() + 1}</span>
            <span class="source-order__name">{props.name(id)}</span>
            <button
              type="button"
              class="icon-btn icon-btn--small"
              aria-label={`Move ${props.name(id)} up`}
              title="Move up (Alt+Up)"
              disabled={index() === 0}
              onClick={() => move(id, -1)}
            >
              <ChevronUp class="icon" aria-hidden="true" />
            </button>
            <button
              type="button"
              class="icon-btn icon-btn--small"
              aria-label={`Move ${props.name(id)} down`}
              title="Move down (Alt+Down)"
              disabled={index() === props.order.length - 1}
              onClick={() => move(id, 1)}
            >
              <ChevronDown class="icon" aria-hidden="true" />
            </button>
          </li>
        )}
      </For>
    </ol>
  );
}

/** Which source's value wins per field, plus refreshing every game's metadata. */
export function SourcePriority(props: { model: SettingsModel }) {
  const [sources] = createResource(() => api.metadataSources());
  const [draft, setDraft] = createSignal<MetadataPriority | null>(null);
  const [open, setOpen] = createSignal<MetadataField | null>(null);
  const [refreshing, setRefreshing] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const ids = () => sources()?.map((s) => s.id) ?? [];
  const name = (id: string) => sources()?.find((s) => s.id === id)?.name ?? id;
  const priority = (): MetadataPriority =>
    draft() ?? props.model.settings()?.metadataPriority ?? { order: [], fields: {} };

  function flush() {
    clearTimeout(timer);
    timer = undefined;
    const next = draft();
    if (next === null) return;
    setDraft(null);
    void props.model.update({ metadataPriority: next });
  }
  onCleanup(flush);

  function change(next: MetadataPriority) {
    setDraft(next);
    clearTimeout(timer);
    timer = setTimeout(flush, SAVE_DELAY_MS);
  }

  async function refreshAll() {
    setRefreshing(true);
    try {
      const queued = await api.refreshAllMetadata();
      store.pushToast(
        "info",
        queued === 0
          ? "No games to refresh"
          : `Refreshing metadata for ${queued} ${queued === 1 ? "game" : "games"} in the background`,
      );
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      setRefreshing(false);
    }
  }

  return (
    <div class="form">
      <h3 class="fieldset__legend">Source priority</h3>
      <p class="muted">
        For each field the first source in this order that knows a value wins. Your own edits always
        come first; data from before this version comes last until it is refreshed.
      </p>
      <Show when={sources()} fallback={<p class="muted">Loading…</p>}>
        <div class="source-priority">
          <nav class="source-priority__nav" aria-label="Fields">
            <button
              type="button"
              class="source-priority__field"
              aria-current={open() === null ? "true" : undefined}
              onClick={() => setOpen(null)}
            >
              <span class="source-priority__label">All fields</span>
              <span class="source-priority__note">Default</span>
            </button>
            <For each={PRIORITY_FIELDS}>
              {(field) => (
                <button
                  type="button"
                  class="source-priority__field"
                  aria-current={open() === field.id ? "true" : undefined}
                  onClick={() => setOpen(field.id)}
                >
                  <span class="source-priority__label">{field.label}</span>
                  <Show when={hasOwnOrder(priority(), field.id)}>
                    <span class="source-priority__note">Own order</span>
                  </Show>
                </button>
              )}
            </For>
          </nav>
          <div class="source-priority__order">
            <Show
              when={PRIORITY_FIELDS.find((f) => f.id === open())}
              fallback={
                <>
                  <p class="field__hint">Used by every field without its own order.</p>
                  <OrderList
                    label="Source order for all fields"
                    order={effectiveOrder(priority(), null, ids())}
                    name={name}
                    onChange={(order) => change(withGlobalOrder(priority(), order))}
                  />
                </>
              }
            >
              {(field) => {
                const own = () => hasOwnOrder(priority(), field().id);
                const order = () => effectiveOrder(priority(), field().id, ids());
                return (
                  <>
                    <label class="check">
                      <input
                        type="checkbox"
                        checked={own()}
                        onChange={(event) =>
                          change(
                            withFieldOrder(
                              priority(),
                              field().id,
                              event.currentTarget.checked ? order() : null,
                            ),
                          )
                        }
                      />
                      {field().label}: use its own order
                    </label>
                    <Show
                      when={own()}
                      fallback={
                        <p class="field__hint">
                          Uses the default order: {order().map(name).join(" › ")}
                        </p>
                      }
                    >
                      <OrderList
                        label={`Source order for ${field().label.toLowerCase()}`}
                        order={order()}
                        name={name}
                        onChange={(next) => change(withFieldOrder(priority(), field().id, next))}
                      />
                    </Show>
                  </>
                );
              }}
            </Show>
          </div>
        </div>
      </Show>
      <div class="form__actions form__actions--start">
        <button
          type="button"
          class="btn btn--with-icon"
          disabled={refreshing()}
          onClick={() => void refreshAll()}
        >
          <RefreshCw class="icon" aria-hidden="true" />
          Refresh metadata for all games
        </button>
      </div>
      <p class="field__hint">
        Asks every store and IGDB again for every game. It runs in the background and can take a
        while for a large library.
      </p>
    </div>
  );
}
