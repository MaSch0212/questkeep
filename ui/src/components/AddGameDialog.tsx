import { createSignal, Show } from "solid-js";
import { addGame } from "../lib/actions";
import { deriveGameName } from "../lib/names";
import { pickDirectory, pickExecutable, pickImage } from "../lib/pickers";
import { Modal } from "./Modal";

function emptyToNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

export function AddGameDialog(props: { onClose: () => void }) {
  const [executable, setExecutable] = createSignal("");
  const [name, setName] = createSignal("");
  const [nameTouched, setNameTouched] = createSignal(false);
  const [args, setArgs] = createSignal("");
  const [workingDir, setWorkingDir] = createSignal("");
  const [cover, setCover] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  async function chooseExecutable() {
    const path = await pickExecutable();
    if (path === null) return;
    setExecutable(path);
    if (!nameTouched()) setName(deriveGameName(path));
  }

  async function chooseDir() {
    const path = await pickDirectory();
    if (path !== null) setWorkingDir(path);
  }

  async function chooseCover() {
    const path = await pickImage();
    if (path !== null) setCover(path);
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (executable() === "") return;
    setBusy(true);
    setError(null);
    const result = await addGame({
      name: emptyToNull(name()),
      executable: executable(),
      args: emptyToNull(args()),
      workingDir: emptyToNull(workingDir()),
      coverImage: cover(),
    });
    setBusy(false);
    if ("error" in result) setError(result.error);
    else props.onClose();
  }

  return (
    <Modal title="Add game" onClose={props.onClose}>
      <form class="form" onSubmit={(event) => void submit(event)}>
        <div class="field">
          <span class="field__label">Executable</span>
          <span class="field__row">
            <input
              class="input"
              readOnly
              aria-label="Executable path"
              placeholder="No file chosen"
              value={executable()}
            />
            <button
              type="button"
              class="btn"
              data-autofocus
              onClick={() => void chooseExecutable()}
            >
              Choose executable…
            </button>
          </span>
        </div>
        <label class="field">
          <span class="field__label">Name</span>
          <input
            class="input"
            value={name()}
            onInput={(event) => {
              setNameTouched(true);
              setName(event.currentTarget.value);
            }}
          />
        </label>
        <label class="field">
          <span class="field__label">Arguments</span>
          <input class="input" value={args()} onInput={(e) => setArgs(e.currentTarget.value)} />
        </label>
        <label class="field">
          <span class="field__label">Working directory (optional)</span>
          <span class="field__row">
            <input
              class="input"
              value={workingDir()}
              onInput={(e) => setWorkingDir(e.currentTarget.value)}
            />
            <button type="button" class="btn" onClick={() => void chooseDir()}>
              Browse…
            </button>
          </span>
        </label>
        <div class="field">
          <span class="field__label">Cover image (optional)</span>
          <span class="field__row">
            <input
              class="input"
              readOnly
              aria-label="Cover image path"
              placeholder="None"
              value={cover() ?? ""}
            />
            <button type="button" class="btn" onClick={() => void chooseCover()}>
              Choose cover…
            </button>
            <Show when={cover() !== null}>
              <button type="button" class="btn" onClick={() => setCover(null)}>
                Clear
              </button>
            </Show>
          </span>
        </div>
        <Show when={error()}>
          <p class="error-text" role="alert">
            {error()}
          </p>
        </Show>
        <div class="form__actions">
          <button type="submit" class="btn btn--primary" disabled={busy() || executable() === ""}>
            Add game
          </button>
          <button type="button" class="btn" onClick={props.onClose}>
            Cancel
          </button>
        </div>
      </form>
    </Modal>
  );
}
