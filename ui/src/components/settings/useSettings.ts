import { createSignal, onMount } from "solid-js";
import type { Settings } from "../../bindings/Settings";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { scanSummaryText } from "../../lib/query";
import { textOf, withSteamSettings } from "../../lib/steamSettings";
import { store } from "../../lib/store";

/** Loaded settings plus the actions that persist them. Shared by all settings sections. */
export function createSettingsModel() {
  const [settings, setSettings] = createSignal<Settings | null>(null);
  const [apiKey, setApiKey] = createSignal("");
  const [steamId, setSteamId] = createSignal("");
  const [savingSteam, setSavingSteam] = createSignal(false);

  onMount(() => {
    api
      .getSettings()
      .then((loaded) => {
        setSettings(loaded);
        setApiKey(textOf(loaded.providers.steam?.apiKey));
        setSteamId(textOf(loaded.providers.steam?.steamId));
      })
      .catch((error: unknown) => store.pushToast("error", errorMessage(error)));
  });

  async function toggle(key: keyof Settings, value: boolean) {
    const previous = settings();
    if (!previous) return;
    setSettings({ ...previous, [key]: value });
    try {
      await api.saveSettings({ ...previous, [key]: value });
    } catch (error) {
      setSettings(previous);
      store.pushToast("error", errorMessage(error));
    }
  }

  async function saveSteam() {
    const previous = settings();
    if (!previous || savingSteam()) return;
    const next = withSteamSettings(previous, apiKey(), steamId());
    const keyChanged =
      textOf(previous.providers.steam?.apiKey) !== textOf(next.providers.steam?.apiKey);
    const idChanged =
      textOf(previous.providers.steam?.steamId) !== textOf(next.providers.steam?.steamId);
    setSavingSteam(true);
    try {
      await api.saveSettings(next);
      setSettings(next);
      setApiKey(textOf(next.providers.steam?.apiKey));
      setSteamId(textOf(next.providers.steam?.steamId));
      store.pushToast("info", "Steam account saved");
    } catch (error) {
      store.pushToast("error", errorMessage(error));
      setSavingSteam(false);
      return;
    }
    setSavingSteam(false);
    // A new key or account can reveal owned games that were not visible before.
    if ((keyChanged || idChanged) && textOf(next.providers.steam?.apiKey) !== "") {
      try {
        const summary = await api.scanLibrary("steam");
        store.pushToast("info", `Steam: ${scanSummaryText(summary)}`);
        store.scheduleRefresh();
      } catch (error) {
        store.pushToast("error", errorMessage(error));
      }
    }
  }

  return { settings, apiKey, setApiKey, steamId, setSteamId, savingSteam, toggle, saveSteam };
}

export type SettingsModel = ReturnType<typeof createSettingsModel>;
