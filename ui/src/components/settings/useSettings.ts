import { createSignal, onMount } from "solid-js";
import type { Settings } from "../../bindings/Settings";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { scanSummaryText } from "../../lib/query";
import { withProviderOption } from "../../lib/providerSettings";
import { textOf, withSteamSettings } from "../../lib/steamSettings";
import { store } from "../../lib/store";

/** Loaded settings plus the actions that persist them. Shared by all settings sections. */
export function createSettingsModel() {
  const [settings, setSettings] = createSignal<Settings | null>(null);
  const [apiKey, setApiKey] = createSignal("");
  const [steamId, setSteamId] = createSignal("");
  const [savingSteam, setSavingSteam] = createSignal(false);
  const [gridDbKey, setGridDbKey] = createSignal("");
  const [savingGridDb, setSavingGridDb] = createSignal(false);
  const [igdbClientId, setIgdbClientId] = createSignal("");
  const [igdbClientSecret, setIgdbClientSecret] = createSignal("");
  const [savingIgdb, setSavingIgdb] = createSignal(false);

  onMount(() => {
    api
      .getSettings()
      .then((loaded) => {
        setSettings(loaded);
        setApiKey(textOf(loaded.providers.steam?.apiKey));
        setSteamId(textOf(loaded.providers.steam?.steamId));
        setGridDbKey(loaded.steamGridDbApiKey ?? "");
        setIgdbClientId(loaded.igdbClientId ?? "");
        setIgdbClientSecret(loaded.igdbClientSecret ?? "");
      })
      .catch((error: unknown) => store.pushToast("error", errorMessage(error)));
  });

  /** Saves a change right away (optimistically) and tells the app shell about it. */
  async function update(patch: Partial<Settings>) {
    const previous = settings();
    if (!previous) return;
    const next = { ...previous, ...patch };
    setSettings(next);
    try {
      await api.saveSettings(next);
      store.applySettings(next);
    } catch (error) {
      setSettings(previous);
      store.pushToast("error", errorMessage(error));
    }
  }

  async function toggle(key: keyof Settings, value: boolean) {
    await update({ [key]: value });
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
      store.pushToast(
        "info",
        textOf(next.providers.steam?.apiKey) === "" ? "Steam key removed" : "Steam key saved",
      );
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

  /** Whether a Steam Web API key is saved (not just typed). */
  const steamKeySaved = () => textOf(settings()?.providers.steam?.apiKey) !== "";

  /** Forgets the Steam Web API key and SteamID. */
  async function removeSteamKey() {
    setApiKey("");
    setSteamId("");
    await saveSteam();
  }

  /**
   * Saves one provider option, then rescans that provider so play actions and the like follow
   * the new value.
   */
  async function setProviderOption(providerId: string, key: string, value: unknown) {
    const previous = settings();
    if (!previous) return;
    const next = withProviderOption(previous, providerId, key, value);
    setSettings(next);
    try {
      await api.saveSettings(next);
    } catch (error) {
      setSettings(previous);
      store.pushToast("error", errorMessage(error));
      return;
    }
    try {
      await api.scanLibrary(providerId);
      store.scheduleRefresh();
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    }
  }

  async function saveGridDb() {
    const previous = settings();
    if (!previous || savingGridDb()) return;
    const key = gridDbKey().trim();
    const next: Settings = { ...previous, steamGridDbApiKey: key === "" ? null : key };
    setSavingGridDb(true);
    try {
      await api.saveSettings(next);
      setSettings(next);
      setGridDbKey(key);
      store.pushToast("info", key === "" ? "SteamGridDB key removed" : "SteamGridDB key saved");
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      setSavingGridDb(false);
    }
  }

  async function saveIgdb() {
    const previous = settings();
    if (!previous || savingIgdb()) return;
    const id = igdbClientId().trim();
    const secret = igdbClientSecret().trim();
    const next: Settings = {
      ...previous,
      igdbClientId: id === "" ? null : id,
      igdbClientSecret: secret === "" ? null : secret,
    };
    setSavingIgdb(true);
    try {
      await api.saveSettings(next);
      setSettings(next);
      setIgdbClientId(id);
      setIgdbClientSecret(secret);
      store.pushToast(
        "info",
        id === "" && secret === "" ? "IGDB credentials removed" : "IGDB credentials saved",
      );
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      setSavingIgdb(false);
    }
  }

  return {
    settings,
    apiKey,
    setApiKey,
    steamId,
    setSteamId,
    savingSteam,
    gridDbKey,
    setGridDbKey,
    savingGridDb,
    igdbClientId,
    setIgdbClientId,
    igdbClientSecret,
    setIgdbClientSecret,
    savingIgdb,
    toggle,
    update,
    setProviderOption,
    saveSteam,
    steamKeySaved,
    removeSteamKey,
    saveGridDb,
    saveIgdb,
  };
}

export type SettingsModel = ReturnType<typeof createSettingsModel>;
