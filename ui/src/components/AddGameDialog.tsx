import { createSignal, For, Show } from "solid-js";
import { Portal } from "solid-js/web";
import type { Game } from "../bindings/Game";
import type { InstalledProgram } from "../bindings/InstalledProgram";
import type { MetadataLink } from "../bindings/MetadataLink";
import { translator } from "../i18n";
import { gameEdit } from "../i18n/gameEdit";
import { gameSetup } from "../i18n/gameSetup";
import { shellAddGame } from "../i18n/shellAddGame";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { deriveGameName } from "../lib/names";
import { pickDirectory, pickExecutable } from "../lib/pickers";
import { store } from "../lib/store";
import { EditGameDialog, SECTIONS, type Section } from "./EditGameDialog";
import { MetadataLinkPicker } from "./MetadataLinkPicker";
import { Modal } from "./Modal";
import { ProgramPicker } from "./ProgramPicker";

const tr = translator(shellAddGame);
const trEdit = translator(gameEdit);
const trSetup = translator(gameSetup);

function emptyToNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/**
 * Add game: the Edit game dialog with a Setup section first (executable, name, arguments and
 * where the metadata comes from). The game is created when the user leaves Setup or saves;
 * Cancel afterwards removes it again.
 */
export function AddGameDialog(props: { onClose: () => void }) {
  const [executable, setExecutable] = createSignal("");
  const [name, setName] = createSignal("");
  const [nameTouched, setNameTouched] = createSignal(false);
  const [args, setArgs] = createSignal("");
  const [workingDir, setWorkingDir] = createSignal("");
  const [link, setLink] = createSignal<MetadataLink | null>(null);
  const [programs, setPrograms] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [created, setCreated] = createSignal<Game | null>(null);
  const [section, setSection] = createSignal<Section>("general");

  function proposeName(value: string) {
    if (!nameTouched()) setName(value);
  }

  async function chooseExecutable() {
    const path = await pickExecutable();
    if (path === null) return;
    setExecutable(path);
    proposeName(deriveGameName(path));
  }

  async function chooseDir() {
    const path = await pickDirectory();
    if (path !== null) setWorkingDir(path);
  }

  function useProgram(program: InstalledProgram) {
    setPrograms(false);
    setExecutable(program.executable);
    setArgs(program.args ?? "");
    setWorkingDir(program.workingDir ?? "");
    proposeName(program.name);
  }

  /** Adds the game with the Setup values; `null` (and the error shown) when that failed. */
  async function create(): Promise<Game | null> {
    if (executable() === "") {
      setError(trSetup("exeRequired"));
      return null;
    }
    setBusy(true);
    setError(null);
    try {
      let game = await api.addManualGame({
        name: emptyToNull(name()),
        executable: executable(),
        args: emptyToNull(args()),
        workingDir: emptyToNull(workingDir()),
        coverImage: null,
      });
      const chosen = link();
      if (chosen !== null) {
        try {
          game = await api.setMetadataLink(game.id, chosen);
        } catch (failure) {
          // The game is added; the link can be chosen again in Setup.
          store.pushToast("error", errorMessage(failure));
        }
      }
      await store.refresh();
      return game;
    } catch (failure) {
      setError(errorMessage(failure));
      return null;
    } finally {
      setBusy(false);
    }
  }

  async function leaveSetup(next: Section) {
    if (next === "setup") return;
    const game = await create();
    if (game === null) return;
    setSection(next);
    setCreated(game);
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const game = await create();
    if (game === null) return;
    store.selectAndOpen(game.id);
    props.onClose();
  }

  /** Cancel after the game was created: it goes again. */
  async function discard(id: string) {
    try {
      await api.removeGame(id);
      await store.refresh();
    } catch (failure) {
      store.pushToast("error", errorMessage(failure));
    }
    props.onClose();
  }

  return (
    <Show
      when={created()}
      fallback={
        <Modal title={tr("addGame")} wide onClose={props.onClose}>
          <form class="edit-game" onSubmit={(event) => void submit(event)}>
            <div class="edit-game__main">
              <nav class="edit-game__nav" aria-label={trEdit("sections")}>
                <For each={SECTIONS}>
                  {(entry) => (
                    <button
                      type="button"
                      class="edit-game__tab"
                      aria-current={entry.id === "setup" ? "page" : undefined}
                      disabled={busy()}
                      onClick={() => void leaveSetup(entry.id)}
                    >
                      {trEdit(entry.label)}
                    </button>
                  )}
                </For>
              </nav>
              <div class="edit-game__pane">
                <div class="edit-section">
                  <button
                    type="button"
                    class="btn add-game__programs"
                    onClick={() => setPrograms(true)}
                  >
                    {trSetup("installedPrograms")}
                  </button>
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
                  <div class="edit-grid">
                    <label class="field field--full">
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
                      <input
                        class="input"
                        value={args()}
                        onInput={(e) => setArgs(e.currentTarget.value)}
                      />
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
                  </div>
                  <h3 class="edit-section__title">{trSetup("metadataTitle")}</h3>
                  <p class="field__hint">{trSetup("metadataHint")}</p>
                  <MetadataLinkPicker
                    query={name()}
                    link={link()}
                    pending
                    onChange={(next) => {
                      setLink(next);
                      if (next !== null) proposeName(next.name);
                    }}
                  />
                </div>
              </div>
            </div>
            <Show when={error()}>
              <p class="error-text edit-game__error" role="alert">
                {error()}
              </p>
            </Show>
            <div class="edit-game__footer">
              <span />
              <span class="edit-game__buttons">
                <button type="button" class="btn" onClick={props.onClose}>
                  {tr("cancel")}
                </button>
                <button
                  type="submit"
                  class="btn btn--primary"
                  disabled={busy() || executable() === ""}
                >
                  {tr("addGame")}
                </button>
              </span>
            </div>
          </form>
          <Show when={programs()}>
            <Portal>
              {/* Outside the dialog, so its focus trap doesn't fight the picker's. */}
              <ProgramPicker onPick={useProgram} onClose={() => setPrograms(false)} />
            </Portal>
          </Show>
        </Modal>
      }
    >
      {(game) => (
        <EditGameDialog
          game={store.games().find((g) => g.id === game().id) ?? game()}
          section={section()}
          title={tr("addGame")}
          saveLabel={tr("addGame")}
          onCancel={() => void discard(game().id)}
          onSaved={() => store.selectAndOpen(game().id)}
          onClose={props.onClose}
        />
      )}
    </Show>
  );
}
