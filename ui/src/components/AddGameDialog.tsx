import { translator } from "../i18n";
import { shellAddGame } from "../i18n/shellAddGame";
import { newGame } from "../lib/editDrafts";
import { store } from "../lib/store";
import { EditGameDialog } from "./EditGameDialog";

const tr = translator(shellAddGame);

/**
 * Add game: the Edit game dialog on a game that doesn't exist yet, opened on Setup (executable,
 * name, arguments and where the metadata comes from). Everything is a draft; the game is created
 * only when the user presses Add game, and Cancel just closes.
 */
export function AddGameDialog(props: { onClose: () => void }) {
  return (
    <EditGameDialog
      game={newGame()}
      isNew
      section="setup"
      title={tr("addGame")}
      saveLabel={tr("addGame")}
      onSaved={(game) => store.selectAndOpen(game.id)}
      onClose={props.onClose}
    />
  );
}
