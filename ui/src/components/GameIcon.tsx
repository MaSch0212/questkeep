import { createEffect, createMemo, createSignal, on, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { store } from "../lib/store";
import { hueFor, initialsFor } from "./Cover";

/** The game's small icon (from its executable or store), or a coloured initials square. */
export function GameIcon(props: { game: Game }) {
  const [failed, setFailed] = createSignal(false);
  const url = createMemo(() => store.iconUrl(props.game));
  createEffect(on(url, () => setFailed(false), { defer: true }));

  return (
    <Show
      when={failed() ? null : url()}
      fallback={
        <span
          class="game-icon game-icon--placeholder"
          style={{ "--qk-tile-hue": hueFor(props.game.name) }}
          aria-hidden="true"
        >
          {initialsFor(props.game.name)}
        </span>
      }
    >
      {(src) => (
        <img
          class="game-icon"
          src={src()}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          onError={() => setFailed(true)}
        />
      )}
    </Show>
  );
}
