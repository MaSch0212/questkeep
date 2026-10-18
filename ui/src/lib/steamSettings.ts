import type { Settings } from "../bindings/Settings";

export function textOf(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/**
 * Settings with the Steam section replaced by the given credentials. Empty values drop their key;
 * other providers and unknown keys inside the Steam section are kept as they are.
 */
export function withSteamSettings(settings: Settings, apiKey: string, steamId: string): Settings {
  const steam: Record<string, unknown> = { ...settings.providers.steam };
  for (const [key, value] of [
    ["apiKey", apiKey.trim()],
    ["steamId", steamId.trim()],
  ] as const) {
    if (value === "") delete steam[key];
    else steam[key] = value;
  }
  return { ...settings, providers: { ...settings.providers, steam } };
}
