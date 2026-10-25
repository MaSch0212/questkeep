import { createSignal, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { savePlayAction, setCover, updateGame } from "../lib/actions";
import { pickImage } from "../lib/pickers";
import { Cover } from "./Cover";
import { createLaunchOptions, LaunchOptionsFields } from "./LaunchOptionsFields";
import { Modal } from "./Modal";

type CoverChange = { kind: "keep" } | { kind: "set"; path: string } | { kind: "remove" };

function fileName(path: string): string {
  return path.split(/[\\/]/).at(-1) ?? path;
}

/**
 * Name, cover and (for manually added games) launch options. Edits are drafts taken from the
 * game when the dialog opens; Save applies the changed ones in order and stops at the first error.
 */
export function EditGameDialog(props: { game: Game; onClose: () => void }) {
  const game = props.game;
  const [name, setName] = createSignal(game.name);
  const [cover, setCoverChange] = createSignal<CoverChange>({ kind: "keep" });
  const [error, setError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);
  const manual = game.source === "manual";
  const launch = createLaunchOptions(game);

  const newCover = () => {
    const change = cover();
    return change.kind === "set" ? change.path : null;
  };
  const hasCover = () =>
    newCover() !== null || (cover().kind === "keep" && game.coverPath !== null);

  async function pickCover() {
    const path = await pickImage();
    if (path !== null) setCoverChange({ kind: "set", path });
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    const trimmed = name().trim();
    if (trimmed === "") {
      setError("Enter a name for the game.");
      return;
    }
    const steps: (() => Promise<string | null>)[] = [];
    if (trimmed !== props.game.name) steps.push(() => updateGame(game.id, { name: trimmed }));
    if (manual && launch.changed()) {
      steps.push(() => savePlayAction(game.id, launch.playAction()));
    }
    const change = cover();
    if (change.kind !== "keep") {
      steps.push(() => setCover(game.id, change.kind === "set" ? change.path : null));
    }
    setError(null);
    setSaving(true);
    const failure = await steps.reduce<Promise<string | null>>(
      (previous, step) => previous.then((failed) => failed ?? step()),
      Promise.resolve(null),
    );
    setSaving(false);
    if (failure) setError(failure);
    else props.onClose();
  }

  return (
    <Modal title="Edit game" onClose={props.onClose}>
      <form class="edit-game" onSubmit={(event) => void save(event)}>
        <div class="edit-game__cover">
          <div class="edit-game__preview">
            <Cover game={cover().kind === "remove" ? { ...game, coverPath: null } : game} />
          </div>
          <Show when={newCover()}>
            {(path) => (
              <p class="edit-game__note" title={path()}>
                New: {fileName(path())}
              </p>
            )}
          </Show>
          <Show when={cover().kind === "remove"}>
            <p class="edit-game__note">The cover will be removed.</p>
          </Show>
          <div class="edit-game__cover-actions">
            <button type="button" class="btn" onClick={() => void pickCover()}>
              Change…
            </button>
            <button
              type="button"
              class="btn"
              disabled={!hasCover()}
              onClick={() => setCoverChange({ kind: "remove" })}
            >
              Remove
            </button>
          </div>
        </div>
        <div class="edit-game__fields">
          <label class="field">
            <span class="field__label">Name</span>
            <input
              class="input"
              data-autofocus
              value={name()}
              onInput={(event) => setName(event.currentTarget.value)}
              ref={(el) => queueMicrotask(() => el.select())}
            />
          </label>
          <Show when={manual}>
            <LaunchOptionsFields options={launch} />
          </Show>
          <Show when={error()}>
            <p class="error-text" role="alert">
              {error()}
            </p>
          </Show>
        </div>
        <div class="form__actions edit-game__actions">
          <button type="button" class="btn" onClick={props.onClose}>
            Cancel
          </button>
          <button
            type="submit"
            class="btn btn--primary"
            disabled={saving() || (manual && launch.target().trim() === "")}
          >
            Save
          </button>
        </div>
      </form>
    </Modal>
  );
}
