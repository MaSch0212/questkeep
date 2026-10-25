import { createEffect, createMemo, createSignal, on, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { store } from "../lib/store";

export function hueFor(name: string): number {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) % 360;
  return hash;
}

export function initialsFor(name: string): string {
  const words = name.split(/\s+/).filter((w) => w.length > 0);
  const letters = words.slice(0, 2).map((w) => w.charAt(0));
  return (letters.join("") || "?").toUpperCase();
}

/** Cover art, or a gradient placeholder when there is none or the image fails to load. */
export function Cover(props: { game: Game }) {
  const [failed, setFailed] = createSignal(false);
  const url = createMemo(() => store.coverUrl(props.game));
  createEffect(on(url, () => setFailed(false), { defer: true }));
  const hue = createMemo(() => hueFor(props.game.name));
  const initials = createMemo(() => initialsFor(props.game.name));

  return (
    <Show
      when={failed() ? null : url()}
      fallback={
        <div class="cover cover--placeholder" style={{ "--qk-tile-hue": hue() }} aria-hidden="true">
          <span class="cover__initials">{initials()}</span>
        </div>
      }
    >
      {(src) => (
        <img
          class="cover cover--image"
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
