import { createSignal, onMount } from "solid-js";
import type { Settings } from "../../bindings/Settings";
import type { StoreCredentials } from "../../bindings/StoreCredentials";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { scanSummaryText } from "../../lib/query";
import {
  credentialsSaved,
  providerText,
  withCredentials,
  withProviderOption,
} from "../../lib/providerSettings";
import { store } from "../../lib/store";

/** A typed key, trimmed; empty means none. */
const keyValue = (text: string) => (text.trim() === "" ? null : text.trim());

/** Loaded settings plus the actions that persist them. Shared by all settings sections. */
export function createSettingsModel() {
  const [settings, setSettings] = createSignal<Settings | null>(null);
  /** Typed but not yet saved credential values: provider id → field key → value. */
  const [drafts, setDrafts] = createSignal<Record<string, Record<string, string>>>({});
  /** The provider whose credentials are being saved. */
  const [savingCredentials, setSavingCredentials] = createSignal<string | null>(null);
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

  /** A credential field as typed, or as saved while untouched. */
  function credentialDraft(providerId: string, key: string): string {
    return drafts()[providerId]?.[key] ?? providerText(settings(), providerId, key);
  }

  function setCredentialDraft(providerId: string, key: string, value: string) {
    setDrafts((all) => ({ ...all, [providerId]: { ...all[providerId], [key]: value } }));
  }

  /** Saves a store's credential form; new credentials sync the store right away. */
  async function saveCredentials(
    providerId: string,
    storeName: string,
    credentials: StoreCredentials,
  ) {
    const previous = settings();
    if (!previous || savingCredentials() !== null) return;
    const values = Object.fromEntries(
      credentials.fields.map((f) => [f.key, credentialDraft(providerId, f.key)]),
    );
    const next = withCredentials(previous, providerId, values);
    const changed = credentials.fields.some(
      (f) => providerText(previous, providerId, f.key) !== providerText(next, providerId, f.key),
    );
    const saved = credentialsSaved(next, providerId, credentials);
    setSavingCredentials(providerId);
    try {
      await api.saveSettings(next);
      setSettings(next);
      setDrafts(({ [providerId]: _saved, ...rest }) => rest);
      store.pushToast("info", `${credentials.noun} ${saved ? "saved" : "removed"}`);
    } catch (error) {
      store.pushToast("error", errorMessage(error));
      setSavingCredentials(null);
      return;
    }
    setSavingCredentials(null);
    // New credentials can reveal owned games that were not visible before.
    if (changed && saved) {
      try {
        const summary = await api.scanLibrary(providerId);
        store.pushToast("info", `${storeName}: ${scanSummaryText(summary)}`);
        store.scheduleRefresh();
      } catch (error) {
        store.pushToast("error", errorMessage(error));
      }
    }
  }

  /** Forgets a store's credentials. */
  async function removeCredentials(
    providerId: string,
    storeName: string,
    credentials: StoreCredentials,
  ) {
    for (const field of credentials.fields) setCredentialDraft(providerId, field.key, "");
    await saveCredentials(providerId, storeName, credentials);
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
      void store.loadSetup();
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

  /**
   * Saves the IGDB and SteamGridDB keys as typed in one go (first-run setup). Returns whether
   * that worked; a failure is shown as a toast.
   */
  async function saveKeys(): Promise<boolean> {
    const previous = settings();
    if (!previous) return false;
    const next: Settings = {
      ...previous,
      igdbClientId: keyValue(igdbClientId()),
      igdbClientSecret: keyValue(igdbClientSecret()),
      steamGridDbApiKey: keyValue(gridDbKey()),
    };
    try {
      await api.saveSettings(next);
      setSettings(next);
      return true;
    } catch (error) {
      store.pushToast("error", errorMessage(error));
      return false;
    }
  }

  return {
    settings,
    saveKeys,
    credentialDraft,
    setCredentialDraft,
    savingCredentials,
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
    saveCredentials,
    removeCredentials,
    saveGridDb,
    saveIgdb,
  };
}

export type SettingsModel = ReturnType<typeof createSettingsModel>;
