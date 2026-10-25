import type { ProviderInfo } from "../bindings/ProviderInfo";

/** One entry of the "Add game" menu: a provider's own way to add games. */
export interface AddGameMenuItem {
  providerId: string;
  providerName: string;
  optionId: string;
  label: string;
}

/** Every provider's add-game options, in provider order. */
export function addGameMenuItems(providers: ProviderInfo[]): AddGameMenuItem[] {
  return providers.flatMap((provider) =>
    (provider.addGameOptions ?? []).map((option) => ({
      providerId: provider.id,
      providerName: provider.name,
      optionId: option.id,
      label: option.label,
    })),
  );
}

/** Index of the item to focus after `key` in a menu of `count` items (null: not a menu key). */
export function nextMenuIndex(key: string, current: number, count: number): number | null {
  if (count === 0) return null;
  switch (key) {
    case "ArrowDown":
      return (current + 1) % count;
    case "ArrowUp":
      return (current - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}
