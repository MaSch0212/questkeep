import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import type { RuntimeId } from "../bindings/RuntimeId";
import type { ScriptVariable } from "../bindings/ScriptVariable";
import { insertAtCursor } from "../lib/fieldInsert";
import type { CodeEditorHandle as Engine } from "../lib/codeMirror";

/** What the owner of an editor can do with it. */
export interface CodeEditorHandle {
  /** Inserts text at the cursor (replacing the selection). */
  insert: (text: string) => void;
  focus: () => void;
}

/**
 * The script editor: CodeMirror 6 with the runtime's syntax highlighting and completions for its
 * keywords and the variables. CodeMirror loads on demand; until it has (or if it can't) a plain
 * text area with the same text stands in.
 */
export function CodeEditor(props: {
  value: string;
  onChange: (text: string) => void;
  runtime: RuntimeId | null;
  variables: readonly ScriptVariable[];
  label: string;
  placeholder?: string;
  /** The editor got the focus (so the variables panel knows where to insert). */
  onFocus?: () => void;
  ref?: (handle: CodeEditorHandle) => void;
}) {
  let host: HTMLDivElement | undefined;
  let fallback: HTMLTextAreaElement | undefined;
  const [engine, setEngine] = createSignal<Engine | null>(null);
  let disposed = false;

  onMount(() => {
    void import("../lib/codeMirror")
      .then((module) => {
        if (disposed || !host) return;
        setEngine(
          module.createCodeEditor({
            parent: host,
            doc: props.value,
            runtime: props.runtime,
            variables: () => props.variables,
            placeholder: props.placeholder,
            label: props.label,
            onChange: (text) => props.onChange(text),
            onFocus: () => props.onFocus?.(),
          }),
        );
      })
      .catch(() => undefined);
  });
  onCleanup(() => {
    disposed = true;
    engine()?.destroy();
  });

  createEffect(() => engine()?.setDoc(props.value));
  createEffect(() => engine()?.setRuntime(props.runtime));

  props.ref?.({
    insert: (text) => {
      const active = engine();
      if (active) active.insert(text);
      else if (fallback) insertAtCursor(fallback, text);
    },
    focus: () => (engine() ? engine()?.focus() : fallback?.focus()),
  });

  return (
    <div class="code-editor">
      <div ref={(el) => (host = el)} class="code-editor__host" hidden={engine() === null} />
      <Show when={engine() === null}>
        <textarea
          ref={(el) => (fallback = el)}
          class="input code-editor__fallback"
          aria-label={props.label}
          placeholder={props.placeholder}
          spellcheck={false}
          value={props.value}
          onInput={(event) => props.onChange(event.currentTarget.value)}
          onFocus={() => props.onFocus?.()}
        />
      </Show>
    </div>
  );
}
