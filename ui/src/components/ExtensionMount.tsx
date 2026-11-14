import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { translator } from "../i18n";
import { extensions } from "../i18n/extensions";
import { errorMessage } from "../lib/errors";
import { guard, logPluginError } from "../lib/extensions/registry";

const tr = translator(extensions);

/**
 * A place where an extension draws its own UI: `render(container)` runs once the container is
 * in the page, the function it returns when the container goes away. A render that throws
 * shows a message instead of breaking the page around it.
 */
export function ExtensionMount(props: {
  plugin: string;
  render: (container: HTMLElement) => void | (() => void);
  class?: string;
}) {
  const [error, setError] = createSignal<string | null>(null);
  let container: HTMLDivElement | undefined;

  onMount(() => {
    if (!container) return;
    let cleanup: void | (() => void);
    try {
      cleanup = props.render(container);
    } catch (failure) {
      logPluginError(props.plugin, "render failed", failure);
      setError(errorMessage(failure));
    }
    const { plugin } = props;
    onCleanup(() => {
      if (typeof cleanup === "function") guard(plugin, "render cleanup failed", cleanup);
    });
  });

  return (
    <>
      <Show when={error()}>
        {(message) => (
          <p class="plugin__error selectable" role="alert">
            {tr("renderFailed", { error: message() })}
          </p>
        )}
      </Show>
      <div
        ref={(el) => (container = el)}
        class={props.class ? `ext-mount ${props.class}` : "ext-mount"}
        data-plugin={props.plugin}
      />
    </>
  );
}
