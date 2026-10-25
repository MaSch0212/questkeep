import { createSignal } from "solid-js";
import type { Game } from "../bindings/Game";
import type { PlayAction } from "../bindings/PlayAction";
import { pickDirectory, pickExecutable } from "../lib/pickers";

function emptyToNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/** Editable launch options of a manually added game; `playAction()` builds the result. */
export function createLaunchOptions(game: Game) {
  const action = game.playAction;
  const [target, setTarget] = createSignal(action?.target ?? "");
  const [args, setArgs] = createSignal(action?.args ?? "");
  const [workingDir, setWorkingDir] = createSignal(action?.workingDir ?? "");

  const playAction = (): PlayAction => ({
    kind: action?.kind ?? "file",
    target: target().trim(),
    args: emptyToNull(args()),
    workingDir: emptyToNull(workingDir()),
    tracking: action?.tracking ?? "default",
    trackingPath: action?.trackingPath ?? null,
  });
  const changed = () => {
    const next = playAction();
    return (
      next.target !== (action?.target ?? "") ||
      next.args !== (action?.args ?? null) ||
      next.workingDir !== (action?.workingDir ?? null)
    );
  };

  return { target, setTarget, args, setArgs, workingDir, setWorkingDir, playAction, changed };
}

export type LaunchOptions = ReturnType<typeof createLaunchOptions>;

export function LaunchOptionsFields(props: { options: LaunchOptions }) {
  async function browseTarget() {
    const path = await pickExecutable();
    if (path !== null) props.options.setTarget(path);
  }

  async function browseDir() {
    const path = await pickDirectory();
    if (path !== null) props.options.setWorkingDir(path);
  }

  return (
    <fieldset class="fieldset" aria-label="Launch options">
      <legend class="fieldset__legend">Launch options</legend>
      <label class="field">
        <span class="field__label">Target path</span>
        <span class="field__row">
          <input
            class="input"
            value={props.options.target()}
            required
            onInput={(e) => props.options.setTarget(e.currentTarget.value)}
          />
          <button type="button" class="btn" onClick={() => void browseTarget()}>
            Browse…
          </button>
        </span>
      </label>
      <label class="field">
        <span class="field__label">Arguments</span>
        <input
          class="input"
          value={props.options.args()}
          onInput={(e) => props.options.setArgs(e.currentTarget.value)}
        />
      </label>
      <label class="field">
        <span class="field__label">Working directory</span>
        <span class="field__row">
          <input
            class="input"
            value={props.options.workingDir()}
            onInput={(e) => props.options.setWorkingDir(e.currentTarget.value)}
          />
          <button type="button" class="btn" onClick={() => void browseDir()}>
            Browse…
          </button>
        </span>
      </label>
    </fieldset>
  );
}
