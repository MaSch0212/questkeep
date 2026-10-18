import { createSignal } from "solid-js";
import type { Game } from "../bindings/Game";
import type { PlayAction } from "../bindings/PlayAction";
import { savePlayAction } from "../lib/actions";
import { pickDirectory, pickExecutable } from "../lib/pickers";

interface LaunchOptionsFormProps {
  game: Game;
  onDone: () => void;
}

function emptyToNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

export function LaunchOptionsForm(props: LaunchOptionsFormProps) {
  const action = props.game.playAction;
  const [target, setTarget] = createSignal(action?.target ?? "");
  const [args, setArgs] = createSignal(action?.args ?? "");
  const [workingDir, setWorkingDir] = createSignal(action?.workingDir ?? "");
  const [error, setError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);

  async function browseTarget() {
    const path = await pickExecutable();
    if (path !== null) setTarget(path);
  }

  async function browseDir() {
    const path = await pickDirectory();
    if (path !== null) setWorkingDir(path);
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    const next: PlayAction = {
      kind: action?.kind ?? "file",
      target: target().trim(),
      args: emptyToNull(args()),
      workingDir: emptyToNull(workingDir()),
      tracking: action?.tracking ?? "default",
      trackingPath: action?.trackingPath ?? null,
    };
    const failure = await savePlayAction(props.game.id, next);
    setSaving(false);
    if (failure) setError(failure);
    else props.onDone();
  }

  return (
    <form class="form" onSubmit={(event) => void submit(event)} aria-label="Launch options">
      <label class="field">
        <span class="field__label">Target path</span>
        <span class="field__row">
          <input
            class="input"
            value={target()}
            required
            onInput={(e) => setTarget(e.currentTarget.value)}
          />
          <button type="button" class="btn" onClick={() => void browseTarget()}>
            Browse…
          </button>
        </span>
      </label>
      <label class="field">
        <span class="field__label">Arguments</span>
        <input class="input" value={args()} onInput={(e) => setArgs(e.currentTarget.value)} />
      </label>
      <label class="field">
        <span class="field__label">Working directory</span>
        <span class="field__row">
          <input
            class="input"
            value={workingDir()}
            onInput={(e) => setWorkingDir(e.currentTarget.value)}
          />
          <button type="button" class="btn" onClick={() => void browseDir()}>
            Browse…
          </button>
        </span>
      </label>
      {error() && (
        <p class="error-text" role="alert">
          {error()}
        </p>
      )}
      <div class="form__actions">
        <button
          type="submit"
          class="btn btn--primary"
          disabled={saving() || target().trim() === ""}
        >
          Save
        </button>
        <button type="button" class="btn" onClick={props.onDone}>
          Cancel
        </button>
      </div>
    </form>
  );
}
