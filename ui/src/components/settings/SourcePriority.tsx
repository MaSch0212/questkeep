import ChevronDown from "lucide-solid/icons/chevron-down";
import ChevronRight from "lucide-solid/icons/chevron-right";
import ChevronUp from "lucide-solid/icons/chevron-up";
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

/** An ordered list of sources, moved with the arrow buttons or Alt+Up/Down. */
function OrderList(props: {
  label: string;
  order: string[];
  name: (id: string) => string;
  onChange: (order: string[]) => void;
}) {
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
            data-source={id}
            tabIndex={0}
            onKeyDown={(event) => {
              if (!event.altKey) return;
              if (event.key === "ArrowUp") move(id, -1);
              else if (event.key === "ArrowDown") move(id, 1);
              else return;
              event.preventDefault();
            }}
          >
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
        <OrderList
          label="Source order for all fields"
          order={effectiveOrder(priority(), null, ids())}
          name={name}
          onChange={(order) => change(withGlobalOrder(priority(), order))}
        />
        <div class="field-orders">
          <p class="field__hint">Fields can use their own order:</p>
          <For each={PRIORITY_FIELDS}>
            {(field) => {
              const own = () => hasOwnOrder(priority(), field.id);
              const order = () => effectiveOrder(priority(), field.id, ids());
              const expanded = () => open() === field.id;
              return (
                <div class="field-order">
                  <button
                    type="button"
                    class="field-order__head"
                    aria-expanded={expanded()}
                    onClick={() => setOpen(expanded() ? null : field.id)}
                  >
                    <Show
                      when={expanded()}
                      fallback={<ChevronRight class="icon" aria-hidden="true" />}
                    >
                      <ChevronDown class="icon" aria-hidden="true" />
                    </Show>
                    <span class="field-order__label">{field.label}</span>
                    <span class="field-order__summary">
                      {own() ? "Own order: " : ""}
                      {order().slice(0, 3).map(name).join(" › ")}
                      {order().length > 3 ? " › …" : ""}
                    </span>
                  </button>
                  <Show when={expanded()}>
                    <div class="field-order__body">
                      <label class="check">
                        <input
                          type="checkbox"
                          checked={own()}
                          onChange={(event) =>
                            change(
                              withFieldOrder(
                                priority(),
                                field.id,
                                event.currentTarget.checked ? order() : null,
                              ),
                            )
                          }
                        />
                        Use its own order
                      </label>
                      <Show when={own()}>
                        <OrderList
                          label={`Source order for ${field.label.toLowerCase()}`}
                          order={order()}
                          name={name}
                          onChange={(next) => change(withFieldOrder(priority(), field.id, next))}
                        />
                      </Show>
                    </div>
                  </Show>
                </div>
              );
            }}
          </For>
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
