import type { Game } from "../bindings/Game";

export type PrimaryAction = "play" | "install" | "running";

/**
 * The one button a game offers on its cover or row. Mirrors `activateGame`: installed games play,
 * not-installed store games install; manual games (no store id) cannot be installed, so they get
 * no button.
 */
export function primaryAction(
  game: Pick<Game, "installed" | "sourceGameId">,
  running: boolean,
): PrimaryAction | null {
  if (running) return "running";
  if (game.installed) return "play";
  return game.sourceGameId !== null ? "install" : null;
}
