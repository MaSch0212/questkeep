import type { Settings } from "../bindings/Settings";
import type { StoreCredentials } from "../bindings/StoreCredentials";

export function textOf(value: unknown): string {
  return typeof value === "string" ? value : "";
}

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

/** A provider's saved text value (`providers.<id>.<key>`), empty when unset. */
export function providerText(settings: Settings | null, providerId: string, key: string): string {
  return textOf(settings?.providers[providerId]?.[key]);
}

/**
 * Settings with a provider's credential fields replaced by the given values (trimmed). Empty
 * values drop their key; other providers and other keys of the provider are kept as they are.
 */
export function withCredentials(
  settings: Settings,
  providerId: string,
  values: Record<string, string>,
): Settings {
  const section: Record<string, unknown> = { ...settings.providers[providerId] };
  for (const [key, raw] of Object.entries(values)) {
    const value = raw.trim();
    if (value === "") delete section[key];
    else section[key] = value;
  }
  return { ...settings, providers: { ...settings.providers, [providerId]: section } };
}

/** Whether every required credential field has a saved value. */
export function credentialsSaved(
  settings: Settings | null,
  providerId: string,
  credentials: StoreCredentials,
): boolean {
  return credentials.fields
    .filter((f) => f.required)
    .every((f) => providerText(settings, providerId, f.key).trim() !== "");
}
