import { Show, type Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import Folder from "lucide-solid/icons/folder";
import Gamepad2 from "lucide-solid/icons/gamepad-2";
import ShoppingBag from "lucide-solid/icons/shopping-bag";
import Store from "lucide-solid/icons/store";
import { sourceInfo, type FallbackGlyph } from "../lib/sources";
import { store } from "../lib/store";

const FALLBACKS: Record<FallbackGlyph, Component<{ class?: string }>> = {
  gamepad: Gamepad2,
  bag: ShoppingBag,
  folder: Folder,
  generic: Store,
};

/** The store's logo glyph (decorative; callers provide the name as text or a title). */
export function SourceIcon(props: { source: string }) {
  const glyph = () => sourceInfo(props.source).glyph;
  return (
    <Show
      when={(() => {
        const g = glyph();
        return g.kind === "path" ? g : undefined;
      })()}
      fallback={(() => {
        const g = glyph();
        return (
          <Dynamic
            component={FALLBACKS[g.kind === "fallback" ? g.icon : "generic"]}
            class="icon"
            aria-hidden="true"
          />
        );
      })()}
    >
      {(brand) => (
        <svg class="source-icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d={brand().path} />
        </svg>
      )}
    </Show>
  );
}

/** Display name of a game's source, from the registered providers. */
export function sourceName(source: string): string {
  return sourceInfo(source, store.providers()).name;
}
