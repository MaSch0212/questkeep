import { createSignal, Show } from "solid-js";
import { translator } from "../i18n";
import { shellAddGame } from "../i18n/shellAddGame";
import { addGame } from "../lib/actions";
import { deriveGameName } from "../lib/names";
import { pickDirectory, pickExecutable, pickImage } from "../lib/pickers";
import { Modal } from "./Modal";

const tr = translator(shellAddGame);

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
    <Modal title={tr("addGame")} onClose={props.onClose}>
      <form class="form" onSubmit={(event) => void submit(event)}>
        <div class="field">
          <span class="field__label">{tr("executable")}</span>
          <span class="field__row">
            <input
              class="input"
              readOnly
              aria-label={tr("executablePath")}
              placeholder={tr("noFile")}
              value={executable()}
            />
            <button
              type="button"
              class="btn"
              data-autofocus
              onClick={() => void chooseExecutable()}
            >
              {tr("chooseExecutable")}
            </button>
          </span>
        </div>
        <label class="field">
          <span class="field__label">{tr("name")}</span>
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
          <span class="field__label">{tr("arguments")}</span>
          <input class="input" value={args()} onInput={(e) => setArgs(e.currentTarget.value)} />
        </label>
        <label class="field">
          <span class="field__label">{tr("workingDir")}</span>
          <span class="field__row">
            <input
              class="input"
              value={workingDir()}
              onInput={(e) => setWorkingDir(e.currentTarget.value)}
            />
            <button type="button" class="btn" onClick={() => void chooseDir()}>
              {tr("browse")}
            </button>
          </span>
        </label>
        <div class="field">
          <span class="field__label">{tr("coverOptional")}</span>
          <span class="field__row">
            <input
              class="input"
              readOnly
              aria-label={tr("coverPath")}
              placeholder={tr("none")}
              value={cover() ?? ""}
            />
            <button type="button" class="btn" onClick={() => void chooseCover()}>
              {tr("chooseCover")}
            </button>
            <Show when={cover() !== null}>
              <button type="button" class="btn" onClick={() => setCover(null)}>
                {tr("clear")}
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
            {tr("addGame")}
          </button>
          <button type="button" class="btn" onClick={props.onClose}>
            {tr("cancel")}
          </button>
        </div>
      </form>
    </Modal>
  );
}
