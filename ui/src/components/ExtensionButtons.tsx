import { For, Show } from "solid-js";
import { eventCount, guard, titleBarButtons, type TitleBarEntry } from "../lib/extensions/registry";
import { ExtensionIcon } from "./ExtensionIcon";

/** The badge text of a button, read again after app events and `refresh()`; null for none. */
function badgeOf(entry: TitleBarEntry): string | null {
  entry.version();
  eventCount();
  if (typeof entry.button.badge !== "function") return null;
  const value = guard(entry.plugin, "title bar badge failed", () => entry.button.badge?.());
  return value === null || value === undefined || value === "" ? null : String(value);
}

/** The title bar buttons extensions added; they behave like the built-in tools. */
export function ExtensionTitleBarButtons() {
  return (
    <For each={titleBarButtons()}>
      {(entry) => {
        const badge = () => badgeOf(entry);
        const label = () => {
          const text = badge();
          return text === null ? entry.button.label : `${entry.button.label} (${text})`;
        };
        return (
          <span class="titlebar__ext">
            <button
              type="button"
              class="titlebar__tool"
              aria-label={label()}
              data-tooltip={entry.button.label}
              data-extension-button={entry.key}
              onClick={(event) => {
                const anchor = event.currentTarget;
                guard(entry.plugin, "title bar button failed", () => entry.button.onClick(anchor));
              }}
            >
              <ExtensionIcon svg={entry.button.icon} />
            </button>
            <Show when={badge()}>
              {(text) => (
                <span class="titlebar__ext-badge" aria-hidden="true">
                  {text()}
                </span>
              )}
            </Show>
          </span>
        );
      }}
    </For>
  );
}
