export interface SourceInfo {
  id: string;
  /** Display name: the provider's own, else the capitalized id. */
  name: string;
  /** The provider's logo as SVG markup; null for sources without a registered provider. */
  icon: string | null;
}

/** Name and logo of a game's source, as the registered `providers` describe it. */
export function sourceInfo(
  id: string,
  providers: readonly { id: string; name: string; icon?: string }[] = [],
): SourceInfo {
  const provider = providers.find((p) => p.id === id);
  return {
    id,
    name: provider?.name ?? id.charAt(0).toUpperCase() + id.slice(1),
    icon: provider?.icon ?? null,
  };
}
