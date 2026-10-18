import type { Settings } from "../bindings/Settings";

/** Key under `providers.gog` that makes installed GOG games start through GOG Galaxy. */
export const GOG_LAUNCH_THROUGH_GALAXY = "launchThroughGalaxy";

/** A boolean provider option (`providers.<id>.<key>`), false when unset or not a boolean. */
export function providerFlag(settings: Settings, providerId: string, key: string): boolean {
  return settings.providers[providerId]?.[key] === true;
}

/** Settings with one provider option set; other providers and keys are kept as they are. */
export function withProviderOption(
  settings: Settings,
  providerId: string,
  key: string,
  value: unknown,
): Settings {
  const section: Record<string, unknown> = { ...settings.providers[providerId], [key]: value };
  return { ...settings, providers: { ...settings.providers, [providerId]: section } };
}
