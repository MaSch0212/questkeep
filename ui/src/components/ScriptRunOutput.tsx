import CircleCheck from "lucide-solid/icons/circle-check";
import CircleX from "lucide-solid/icons/circle-x";
import Copy from "lucide-solid/icons/copy";
import LoaderCircle from "lucide-solid/icons/loader-circle";
import RotateCcw from "lucide-solid/icons/rotate-ccw";
import Square from "lucide-solid/icons/square";
import { createEffect, For, on, Show } from "solid-js";
import { translator } from "../i18n";
import { scriptsText } from "../i18n/scripts";
import { outputText, type ScriptRun, succeeded, tokenStyle } from "../lib/scriptRun";
import { store } from "../lib/store";
import { Popover, type PopoverAnchor } from "./Popover";

const tr = translator(scriptsText);

/** How close to the bottom (px) the output counts as following new lines. */
const FOLLOW_SLACK = 24;

/**
 * The output of a script run from the script dialog: what it wrote (errors in their own color),
 * whether it is still running, its exit code or why it failed, and Stop / Run again.
 */
export function ScriptRunOutput(props: {
  anchor: PopoverAnchor;
  run: ScriptRun;
  onStop: () => void;
  onAgain: () => void;
  onClose: (returnFocus: boolean) => void;
  /** Elements that don't close the popover when clicked (the buttons that open it). */
  inside?: () => (Element | undefined)[];
}) {
  let output: HTMLPreElement | undefined;
  let follow = true;

  // New output scrolls into view while the reader is at the bottom.
  createEffect(
    on(
      () => props.run.output,
      () => {
        if (follow && output) output.scrollTop = output.scrollHeight;
      },
    ),
  );

  async function copy() {
    try {
      await navigator.clipboard.writeText(outputText(props.run));
      store.pushToast("info", tr("runCopied"));
    } catch {
      // The clipboard can refuse; nothing to do.
    }
  }

  const status = () => {
    const run = props.run;
    if (!run.ended) return tr("runRunning");
    if (run.error !== null) return run.error;
    return tr("runExitCode", { code: run.exitCode ?? "?" });
  };

  return (
    <Popover
      anchor={props.anchor}
      class="run-output"
      role="dialog"
      label={tr("runOutputLabel")}
      inside={props.inside}
      onClose={props.onClose}
    >
      <header class="run-output__head">
        <b class="run-output__title">
          {tr("runTitle", { name: props.run.name, game: props.run.game.name })}
        </b>
        <span
          class="run-output__status"
          classList={{
            "run-output__status--ok": succeeded(props.run),
            "run-output__status--failed": props.run.ended && !succeeded(props.run),
          }}
          role="status"
        >
          <Show
            when={props.run.ended}
            fallback={<LoaderCircle class="icon icon--spin" aria-hidden="true" />}
          >
            <Show
              when={succeeded(props.run)}
              fallback={<CircleX class="icon" aria-hidden="true" />}
            >
              <CircleCheck class="icon" aria-hidden="true" />
            </Show>
          </Show>
          {status()}
        </span>
      </header>
      <pre
        ref={(el) => (output = el)}
        class="run-output__text"
        tabindex="0"
        aria-label={tr("runOutputLabel")}
        onScroll={(event) => {
          const el = event.currentTarget;
          follow = el.scrollHeight - el.scrollTop - el.clientHeight < FOLLOW_SLACK;
        }}
      >
        <Show
          when={props.run.output.length > 0}
          fallback={
            <span class="run-output__empty">{props.run.ended ? tr("runNoOutput") : ""}</span>
          }
        >
          <For each={props.run.output}>
            {(line) => (
              <span classList={{ "run-output__stderr": line.stream === "stderr" }}>
                <For each={line.tokens}>
                  {(token) => <span style={tokenStyle(token)}>{token.value}</span>}
                </For>
              </span>
            )}
          </For>
        </Show>
      </pre>
      <p class="run-output__hint">{tr("runHint")}</p>
      <footer class="run-output__foot">
        <Show
          when={props.run.ended}
          fallback={
            <button type="button" class="btn btn--danger" onClick={() => props.onStop()}>
              <Square class="icon" aria-hidden="true" />
              {tr("runStop")}
            </button>
          }
        >
          <button type="button" class="btn" onClick={() => props.onAgain()}>
            <RotateCcw class="icon" aria-hidden="true" />
            {tr("runAgain")}
          </button>
        </Show>
        <button
          type="button"
          class="btn btn--ghost"
          disabled={props.run.output.length === 0}
          onClick={() => void copy()}
        >
          <Copy class="icon" aria-hidden="true" />
          {tr("runCopy")}
        </button>
        <button
          type="button"
          class="btn btn--ghost run-output__close"
          onClick={() => props.onClose(true)}
        >
          {tr("runClose")}
        </button>
      </footer>
    </Popover>
  );
}
