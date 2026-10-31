import { convertFileSrc } from "@tauri-apps/api/core";
import Check from "lucide-solid/icons/check";
import Search from "lucide-solid/icons/search";
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import type { ImageOption } from "../bindings/ImageOption";
import type { ImageSlot } from "../bindings/ImageSlot";
import type { ImageSource } from "../bindings/ImageSource";
import * as api from "../lib/api";
import { webQuery } from "../lib/editGame";
import { errorMessage } from "../lib/errors";
import { Modal } from "./Modal";

const TITLES: Record<ImageSlot, string> = {
  cover: "Choose a cover",
  background: "Choose a background",
  icon: "Choose an icon",
};

const WEB = "web";
const ALL = "all";

/** An image address the webview can show: web addresses as they are, local files converted. */
export function imageSrc(url: string): string {
  return /^https?:\/\//i.test(url) ? url : convertFileSrc(url);
}

interface Loaded {
  source: ImageSource;
  options: ImageOption[] | null;
  error: string | null;
}

/**
 * Pick a cover, background or icon from the game's stores, IGDB, SteamGridDB or a web search.
 * Sources load in parallel when the picker opens and fill in as they answer; the web search
 * runs only when its tab is opened or Search is pressed.
 */
export function ImagePicker(props: {
  gameId: string;
  gameName: string;
  slot: ImageSlot;
  /** The image the game has now (a local path), shown first. */
  current: string | null;
  onPick: (url: string) => void;
  onClose: () => void;
}) {
  const [loaded, setLoaded] = createSignal<Loaded[]>([]);
  const [sourcesError, setSourcesError] = createSignal<string | null>(null);
  const [tab, setTab] = createSignal(ALL);
  const [selected, setSelected] = createSignal<ImageOption | null>(null);
  const [sizes, setSizes] = createSignal<Record<string, string>>({});
  const [query, setQuery] = createSignal(webQuery(props.gameName, props.slot));
  const [large, setLarge] = createSignal(false);
  const [web, setWeb] = createSignal<ImageOption[] | null>(null);
  const [webError, setWebError] = createSignal<string | null>(null);
  const [searching, setSearching] = createSignal(false);
  let searched = false;

  onMount(() => {
    api
      .imageSources(props.gameId, props.slot)
      .then((sources) => {
        setLoaded(sources.map((source) => ({ source, options: null, error: null })));
        for (const source of sources) {
          api
            .imageOptions(props.gameId, props.slot, source.id)
            .then((options) => update(source.id, options, null))
            .catch((error: unknown) => update(source.id, [], errorMessage(error)));
        }
      })
      .catch((error: unknown) => setSourcesError(errorMessage(error)));
  });

  function update(id: string, options: ImageOption[], error: string | null) {
    setLoaded((list) =>
      list.map((entry) => (entry.source.id === id ? { ...entry, options, error } : entry)),
    );
  }

  async function search() {
    searched = true;
    setSearching(true);
    setWebError(null);
    try {
      setWeb(await api.searchWebImages(query(), large(), props.slot === "icon"));
    } catch (error) {
      setWeb([]);
      setWebError(errorMessage(error));
    } finally {
      setSearching(false);
    }
  }

  function openTab(id: string) {
    setTab(id);
    if (id === WEB && !searched) void search();
  }

  const all = createMemo(() => loaded().flatMap((entry) => entry.options ?? []));
  const pending = () => loaded().some((entry) => entry.options === null);
  const shown = createMemo((): ImageOption[] => {
    const current = tab();
    if (current === WEB) return web() ?? [];
    if (current === ALL) return all();
    return loaded().find((entry) => entry.source.id === current)?.options ?? [];
  });
  const tabError = () => {
    const current = tab();
    if (current === WEB) return webError();
    if (current === ALL) return sourcesError();
    return loaded().find((entry) => entry.source.id === current)?.error ?? null;
  };
  const showCurrent = () => tab() === ALL && props.current !== null;

  const sizeOf = (option: ImageOption) =>
    option.width !== null && option.height !== null
      ? `${option.width} × ${option.height}`
      : (sizes()[option.url] ?? null);
  const footer = () => {
    const option = selected();
    if (!option) return null;
    const size = sizeOf(option);
    return size ? `${size} · ${option.label}` : option.label;
  };

  function measure(option: ImageOption, img: HTMLImageElement) {
    // Thumbnails are smaller than the image; only the image itself tells its real size.
    if (option.thumbnail !== null || option.width !== null) return;
    setSizes((known) => ({ ...known, [option.url]: `${img.naturalWidth} × ${img.naturalHeight}` }));
  }

  function onGridKey(event: KeyboardEvent) {
    if (event.key === "Enter" && selected()) {
      event.preventDefault();
      use();
    }
  }

  function use() {
    const option = selected();
    if (option) props.onPick(option.url);
  }

  return (
    <Modal title={TITLES[props.slot]} wide onClose={props.onClose}>
      <div class={`image-picker image-picker--${props.slot}`}>
        <div class="image-picker__tabs segmented" role="tablist" aria-label="Image sources">
          <button
            type="button"
            role="tab"
            class="segmented__option"
            aria-selected={tab() === ALL}
            onClick={() => openTab(ALL)}
          >
            All {all().length}
          </button>
          <For each={loaded()}>
            {(entry) => (
              <button
                type="button"
                role="tab"
                class="segmented__option"
                aria-selected={tab() === entry.source.id}
                onClick={() => openTab(entry.source.id)}
              >
                {entry.source.name} {entry.options === null ? "…" : entry.options.length}
              </button>
            )}
          </For>
          <button
            type="button"
            role="tab"
            class="segmented__option"
            aria-selected={tab() === WEB}
            onClick={() => openTab(WEB)}
          >
            <Search class="icon icon--xs" aria-hidden="true" /> Web
          </button>
        </div>
        <Show when={tab() === WEB}>
          <form
            class="image-picker__search"
            onSubmit={(event) => {
              event.preventDefault();
              void search();
            }}
          >
            <input
              class="input"
              aria-label="Search the web for images"
              value={query()}
              onInput={(event) => setQuery(event.currentTarget.value)}
            />
            <div class="segmented" role="radiogroup" aria-label="Image size">
              <button
                type="button"
                role="radio"
                class="segmented__option"
                aria-checked={!large()}
                onClick={() => setLarge(false)}
              >
                Any size
              </button>
              <button
                type="button"
                role="radio"
                class="segmented__option"
                aria-checked={large()}
                onClick={() => setLarge(true)}
              >
                Large
              </button>
            </div>
            <button type="submit" class="btn btn--primary" disabled={searching()}>
              Search
            </button>
          </form>
        </Show>
        <Show when={tabError()}>
          <p class="error-text" role="alert">
            {tabError()}
          </p>
        </Show>
        <div
          class="image-picker__grid"
          role="listbox"
          aria-label="Images"
          tabindex="0"
          onKeyDown={onGridKey}
        >
          <Show when={showCurrent()}>
            <div class="image-picker__item image-picker__item--current" aria-hidden="true">
              <img src={imageSrc(props.current ?? "")} alt="" />
              <small>
                <Check class="icon icon--xs" aria-hidden="true" /> Current
              </small>
            </div>
          </Show>
          <For each={shown()}>
            {(option) => (
              <button
                type="button"
                role="option"
                class="image-picker__item"
                aria-selected={selected()?.url === option.url}
                aria-label={option.label}
                onClick={() => setSelected(option)}
                onDblClick={() => {
                  setSelected(option);
                  use();
                }}
              >
                <img
                  src={imageSrc(option.thumbnail ?? option.url)}
                  alt=""
                  loading="lazy"
                  onLoad={(event) => measure(option, event.currentTarget)}
                  onError={(event) => event.currentTarget.parentElement?.classList.add("is-broken")}
                />
                <small>
                  {sizeOf(option) ? `${sizeOf(option)} · ${option.label}` : option.label}
                </small>
              </button>
            )}
          </For>
          <Show when={(tab() === WEB ? searching() : pending()) && shown().length === 0}>
            <div class="image-picker__item image-picker__item--loading">
              <small>Loading…</small>
            </div>
          </Show>
        </div>
        <Show
          when={
            shown().length === 0 &&
            !(tab() === WEB ? searching() : pending()) &&
            !tabError() &&
            !showCurrent()
          }
        >
          <p class="muted image-picker__empty">
            {tab() === WEB ? "No images found." : "No images from these sources."}
          </p>
        </Show>
        <div class="image-picker__footer">
          <span class="field__hint">
            {footer() ??
              (tab() === WEB
                ? "Results from DuckDuckGo image search. Images belong to their owners."
                : "Select an image.")}
          </span>
          <span class="image-picker__actions">
            <button type="button" class="btn" onClick={props.onClose}>
              Cancel
            </button>
            <button type="button" class="btn btn--primary" disabled={!selected()} onClick={use}>
              Use this image
            </button>
          </span>
        </div>
      </div>
    </Modal>
  );
}
