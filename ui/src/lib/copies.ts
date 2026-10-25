import type { Game } from "../bindings/Game";
import type { GameCopy } from "../bindings/GameCopy";

/** A game's store copies in play order; a game the core sent without them is its own copy. */
export function copiesOf(game: Game): GameCopy[] {
  if (game.copies.length > 0) return game.copies;
  const { id, source, sourceGameId, installed, playtimeSeconds, lastPlayed } = game;
  return [{ id, source, sourceGameId, installed, playtimeSeconds, lastPlayed }];
}

/** The stores a game is in, each once, in play order. */
export function storesOf(game: Game): string[] {
  return [...new Set(copiesOf(game).map((c) => c.source))];
}

/** The copy Play starts: the first installed one, else the first. */
export function playCopy(game: Game): GameCopy | undefined {
  const copies = copiesOf(game);
  return copies.find((c) => c.installed) ?? copies[0];
}
