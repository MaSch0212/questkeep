import Puzzle from "lucide-solid/icons/puzzle";
import { createEffect, createSignal, on, Show } from "solid-js";

/**
 * A plugin's icon (an image URL: the registry's data URL, a plugin file, or the install preview's
 * data URL), or a generic puzzle piece without one or when the image does not load. Always an
 * `<img>`, never inlined, so an SVG icon cannot run scripts.
 */
export function PluginIcon(props: { src: string | null; large?: boolean }) {
  const [failed, setFailed] = createSignal(false);
  // A new image (a reloaded plugin) gets a fresh chance.
  createEffect(
    on(
      () => props.src,
      () => setFailed(false),
      { defer: true },
    ),
  );
  return (
    <span class="plugin-icon" classList={{ "plugin-icon--large": props.large === true }}>
      <Show
        when={failed() ? null : props.src}
        fallback={<Puzzle class="icon" aria-hidden="true" />}
      >
        {(src) => <img src={src()} alt="" onError={() => setFailed(true)} />}
      </Show>
    </span>
  );
}
