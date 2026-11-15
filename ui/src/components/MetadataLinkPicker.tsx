import { createResource, createSignal, For, Match, Show, Switch } from "solid-js";
import type { CatalogHit } from "../bindings/CatalogHit";
import type { MetadataLink } from "../bindings/MetadataLink";
import { translator } from "../i18n";
import { gameSetup } from "../i18n/gameSetup";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";

const tr = translator(gameSetup);

/**
 * Finds a game added by hand on IGDB or in a store's catalog and links it, so its metadata and
 * images come from there. `pending` words the link as one that applies once the game is added.
 */
export function MetadataLinkPicker(props: {
  /** What to search for at first (the game's name). */
  query: string;
  link: MetadataLink | null;
  pending?: boolean;
  onChange: (link: MetadataLink | null) => void;
}) {
  const [sources] = createResource(() =>
    Promise.resolve()
      .then(() => api.metadataSearchSources())
      .catch(() => []),
  );
  const [source, setSource] = createSignal<string | null>(null);
  const chosenSource = () => source() ?? sources()?.[0]?.id ?? null;
  const [query, setQuery] = createSignal<string | null>(null);
  const text = () => query() ?? props.query;
  const [hits, setHits] = createSignal<CatalogHit[] | null>(null);
  const [searching, setSearching] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const sourceName = (id: string) => sources()?.find((s) => s.id === id)?.name ?? id;

  async function search() {
    const from = chosenSource();
    if (from === null || text().trim() === "") return;
    setSearching(true);
    setError(null);
    try {
      setHits(await api.searchMetadata(from, text()));
    } catch (failure) {
      setHits(null);
      setError(errorMessage(failure));
    } finally {
      setSearching(false);
    }
  }

  const isChosen = (hit: CatalogHit) =>
    props.link?.source === chosenSource() && props.link?.id === hit.id;

  return (
    <div class="link-picker">
      <Show when={props.link}>
        {(link) => (
          <div class="link-picker__current">
            <span>
              {tr(props.pending ? "linkPending" : "linkedTo", {
                source: sourceName(link().source),
                name: link().name,
              })}
            </span>
            <button
              type="button"
              class="btn btn--sm btn--ghost"
              onClick={() => props.onChange(null)}
            >
              {tr("unlink")}
            </button>
          </div>
        )}
      </Show>
      <Show
        when={(sources() ?? []).length > 0}
        fallback={
          <Show when={sources() !== undefined}>
            <p class="field__hint">{tr("noSources")}</p>
          </Show>
        }
      >
        <div class="link-picker__search">
          <select
            class="input link-picker__source"
            aria-label={tr("source")}
            value={chosenSource() ?? ""}
            onChange={(event) => {
              setSource(event.currentTarget.value);
              setHits(null);
            }}
          >
            <For each={sources()}>{(entry) => <option value={entry.id}>{entry.name}</option>}</For>
          </select>
          <input
            class="input"
            type="search"
            aria-label={tr("searchFor")}
            value={text()}
            onInput={(event) => setQuery(event.currentTarget.value)}
            onKeyDown={(event) => {
              // Inside the Add game form Enter would add the game.
              if (event.key === "Enter") {
                event.preventDefault();
                void search();
              }
            }}
          />
          <button
            type="button"
            class="btn"
            disabled={searching() || text().trim() === ""}
            onClick={() => void search()}
          >
            {tr("search")}
          </button>
        </div>
        <Switch>
          <Match when={searching()}>
            <p class="muted">{tr("searching")}</p>
          </Match>
          <Match when={error()}>
            <p class="error-text" role="alert">
              {error()}
            </p>
          </Match>
          <Match when={hits()?.length === 0}>
            <p class="muted">{tr("noHits")}</p>
          </Match>
          <Match when={hits()}>
            {(list) => (
              <ul class="link-picker__hits" aria-label={tr("searchFor")}>
                <For each={list()}>
                  {(hit) => (
                    <li>
                      <button
                        type="button"
                        class="link-picker__hit"
                        aria-pressed={isChosen(hit)}
                        onClick={() => {
                          const from = chosenSource();
                          if (from !== null) {
                            props.onChange({ source: from, id: hit.id, name: hit.name });
                          }
                        }}
                      >
                        <span class="link-picker__thumb">
                          <Show when={hit.image}>
                            {(src) => (
                              <img
                                src={src()}
                                alt=""
                                loading="lazy"
                                onError={(event) => event.currentTarget.remove()}
                              />
                            )}
                          </Show>
                        </span>
                        <span class="link-picker__name">
                          <span>{hit.name}</span>
                          <small class="muted">
                            {[hit.releaseYear, isChosen(hit) ? tr("chosen") : null]
                              .filter((part) => part !== null)
                              .join(" · ")}
                          </small>
                        </span>
                      </button>
                    </li>
                  )}
                </For>
              </ul>
            )}
          </Match>
        </Switch>
      </Show>
    </div>
  );
}
