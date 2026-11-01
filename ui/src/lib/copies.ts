import type { Game } from "../bindings/Game";
import type { GameCopy } from "../bindings/GameCopy";
import type { ProviderInfo } from "../bindings/ProviderInfo";

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

/** The copies whose store can show the game in its launcher ("Show in Steam"), in play order. */
export function launcherPageCopies(game: Game, providers: readonly ProviderInfo[]): GameCopy[] {
  return copiesOf(game).filter(
    (copy) =>
      copy.sourceGameId !== null && providers.some((p) => p.id === copy.source && p.launcherPages),
  );
}

/** The copy Play starts: the first installed one, else the first. */
export function playCopy(game: Game): GameCopy | undefined {
  const copies = copiesOf(game);
  return copies.find((c) => c.installed) ?? copies[0];
}
