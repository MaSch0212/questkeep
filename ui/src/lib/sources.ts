import {
  siEpicgames,
  siGogdotcom,
  siHeroicgameslauncher,
  siLutris,
  siSteam,
  siUbisoft,
} from "simple-icons";

/** Names of the lucide fallback glyphs (mapped to components in `components/SourceIcon.tsx`). */
export type FallbackGlyph = "gamepad" | "bag" | "folder" | "generic";

export type SourceGlyph =
  /** A monochrome brand glyph from Simple Icons (CC0): one SVG path in a 24x24 box. */
  { kind: "path"; path: string; title: string } | { kind: "fallback"; icon: FallbackGlyph };

export interface SourceInfo {
  id: string;
  /** Display name: the provider's own, else a built-in one, else the raw id. */
  name: string;
  glyph: SourceGlyph;
}

const brand = (icon: { path: string; title: string }): SourceGlyph => ({
  kind: "path",
  path: icon.path,
  title: icon.title,
});

/**
 * Simple Icons has Steam, Epic Games, GOG.com, Ubisoft, Lutris and Heroic; it has no Xbox and no
 * Amazon Games glyph (brand removals), so those use lucide fallbacks.
 */
const KNOWN: Record<string, { name: string; glyph: SourceGlyph }> = {
  steam: { name: "Steam", glyph: brand(siSteam) },
  epic: { name: "Epic Games", glyph: brand(siEpicgames) },
  gog: { name: "GOG", glyph: brand(siGogdotcom) },
  ubisoft: { name: "Ubisoft Connect", glyph: brand(siUbisoft) },
  heroic: { name: "Heroic", glyph: brand(siHeroicgameslauncher) },
  lutris: { name: "Lutris", glyph: brand(siLutris) },
  xbox: { name: "Xbox", glyph: { kind: "fallback", icon: "gamepad" } },
  amazon: { name: "Amazon Games", glyph: { kind: "fallback", icon: "bag" } },
  manual: { name: "Manual", glyph: { kind: "fallback", icon: "folder" } },
};

/** Name and glyph of a game's source; `providers` supplies the authoritative display names. */
export function sourceInfo(
  id: string,
  providers: readonly { id: string; name: string }[] = [],
): SourceInfo {
  const known = KNOWN[id];
  const name = providers.find((p) => p.id === id)?.name ?? known?.name ?? id;
  return { id, name, glyph: known?.glyph ?? { kind: "fallback", icon: "generic" } };
}
