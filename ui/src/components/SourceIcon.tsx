import { Show } from "solid-js";
import Store from "lucide-solid/icons/store";
import { sourceInfo } from "../lib/sources";
import { store } from "../lib/store";

/**
 * The store's logo (decorative; callers provide the name as text or a title). The SVG comes from
 * the provider in the core, compiled into the app, so inserting it as markup is safe.
 */
export function SourceIcon(props: { source: string }) {
  const icon = () => sourceInfo(props.source, store.providers()).icon;
  return (
    <Show when={icon()} fallback={<Store class="icon" aria-hidden="true" />}>
      {(svg) => <span class="source-icon" aria-hidden="true" innerHTML={svg()} />}
    </Show>
  );
}

/** Display name of a game's source, from the registered providers. */
export function sourceName(source: string): string {
  return sourceInfo(source, store.providers()).name;
}
