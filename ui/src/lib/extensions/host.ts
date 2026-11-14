import { createSignal } from "solid-js";
import type { PluginInfo } from "../../bindings/PluginInfo";
import { translator } from "../../i18n";
import { extensions } from "../../i18n/extensions";
import * as app from "../api";
import { errorMessage } from "../errors";
import { store } from "../store";
import { createApi } from "./api";
import { parseExtension } from "./declaredSettings";
import {
  disposePlugin,
  guard,
  logPluginError,
  onRegistryReset,
  registerSettings,
  resetRegistry,
  track,
} from "./registry";
import { createSettingsController } from "./settings";
import type { ExtensionModule } from "./types";

const tr = translator(extensions);

export interface ExtensionState {
  status: "loading" | "active" | "failed";
  /** Why it failed. */
  error?: string;
}

/** Imports an extension's main module by URL. */
export type Importer = (url: string) => Promise<unknown>;
const importModule: Importer = (url) => import(/* @vite-ignore */ url);
/**
 * The file's URL with the plugin's load counter: the webview caches modules by URL. The counter
 * is also a path segment (`@<revision>/`, which the shell skips), so modules the file imports by
 * a relative path are fresh after a reload too.
 */
const fresh = (plugin: PluginInfo, path: string) =>
  `${app.pluginFileUrl(plugin.id, `@${plugin.revision}/${path}`)}?v=${plugin.revision}`;
let importer = importModule;
/** Replaces how modules are imported (tests); null brings back the real import. */
export function setImporter(replacement: Importer | null) {
  importer = replacement ?? importModule;
}

/** How long `activate` and `deactivate` may take before the host stops waiting for them. */
const ACTIVATE_TIMEOUT_MS = 30_000;
const DEACTIVATE_TIMEOUT_MS = 5_000;

interface Instance {
  /** `<plugin id>@<revision>`: another key means the plugin was loaded again. */
  key: string;
  deactivate?: () => unknown;
  cleanup?: () => void;
}

/** The extension instance per plugin id (also one that failed, so it is not retried). */
const instances = new Map<string, Instance>();
/** The plugins whose extension should run, as of the last `syncExtensions`. */
const wanted = new Map<string, PluginInfo>();
/** The pending work per plugin id: loads and unloads of one plugin run one after the other. */
const queues = new Map<string, Promise<void>>();

const [extensionStates, setStates] = createSignal<Readonly<Record<string, ExtensionState>>>({});
/** Whether each plugin's extension runs, by plugin id (plugins without one are missing). */
export { extensionStates };

function setState(id: string, state: ExtensionState | null) {
  setStates(({ [id]: _old, ...rest }) => (state === null ? rest : { ...rest, [id]: state }));
}

const keyOf = (plugin: PluginInfo) => `${plugin.id}@${plugin.revision}`;

/** The promise's result, or a rejection when it takes longer than `ms`. */
function within<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(message)), ms);
    promise.then(resolve, reject).finally(() => clearTimeout(timer));
  });
}

/** The object with `activate`: the module itself or its default export. */
function extensionOf(module: unknown): ExtensionModule | null {
  for (const candidate of [module, (module as { default?: unknown } | null)?.default]) {
    if (
      typeof candidate === "object" &&
      candidate !== null &&
      typeof (candidate as ExtensionModule).activate === "function"
    ) {
      return candidate as ExtensionModule;
    }
  }
  return null;
}

function addStyleSheet(pluginId: string, url: string) {
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = url;
  link.dataset.plugin = pluginId;
  document.head.append(link);
  track(pluginId, () => link.remove());
}

async function load(plugin: PluginInfo): Promise<void> {
  const { id } = plugin;
  const instance: Instance = { key: keyOf(plugin) };
  instances.set(id, instance);
  setState(id, { status: "loading" });
  try {
    const manifest = parseExtension(plugin.extension);
    if (!manifest) throw new Error(tr("noMain"));
    // A stubbed command (tests) may answer with nothing.
    const stored = await app.getPluginSettings(id);
    const settings = createSettingsController({
      declared: manifest.settings,
      initial: stored,
      save: (values) => app.savePluginSettings(id, values),
      onListenerError: (error) => logPluginError(id, "settings listener failed", error),
    });
    registerSettings(id, settings);
    for (const path of manifest.styles) addStyleSheet(id, fresh(plugin, path));
    const extension = extensionOf(await importer(fresh(plugin, manifest.main)));
    if (!extension) throw new Error(tr("noActivate"));
    if (typeof extension.deactivate === "function") {
      instance.deactivate = () => extension.deactivate?.();
    }
    const result = await within(
      Promise.resolve(extension.activate(createApi(plugin, settings))),
      ACTIVATE_TIMEOUT_MS,
      tr("activateTimeout"),
    );
    if (typeof result === "function") instance.cleanup = result;
    else if (result && typeof result.dispose === "function") {
      instance.cleanup = () => result.dispose();
    }
    setState(id, { status: "active" });
  } catch (error) {
    logPluginError(id, "the extension failed to start", error);
    instance.deactivate = undefined;
    disposePlugin(id);
    const message = errorMessage(error);
    setState(id, { status: "failed", error: message });
    store.pushToast("error", tr("failedToast", { name: plugin.name, error: message }));
  }
}

async function unload(id: string, instance: Instance): Promise<void> {
  const { deactivate, cleanup } = instance;
  if (deactivate) {
    try {
      await within(Promise.resolve(deactivate()), DEACTIVATE_TIMEOUT_MS, "deactivate() timed out");
    } catch (error) {
      logPluginError(id, "deactivate failed", error);
    }
  }
  if (cleanup) guard(id, "the cleanup returned by activate failed", cleanup);
  disposePlugin(id);
  instances.delete(id);
}

/** Brings one plugin's extension to what is wanted now (not what was when this was queued). */
async function reconcile(id: string): Promise<void> {
  const target = wanted.get(id);
  const current = instances.get(id);
  if (current?.key === (target && keyOf(target))) return;
  if (current) await unload(id, current);
  if (target) await load(target);
  else setState(id, null);
}

/**
 * Starts, restarts and stops extensions so they match the installed plugins: one instance per
 * enabled, loaded plugin with an extension, loaded again whenever its revision changes.
 * Resolves once every plugin's extension is settled (active, failed or gone); never rejects.
 */
export function syncExtensions(plugins: readonly PluginInfo[] | null | undefined): Promise<void> {
  const ids = new Set([...wanted.keys(), ...instances.keys()]);
  wanted.clear();
  for (const plugin of plugins ?? []) {
    if (plugin.extension && plugin.loaded && plugin.enabled) wanted.set(plugin.id, plugin);
  }
  for (const id of wanted.keys()) ids.add(id);
  const runs: Promise<void>[] = [];
  for (const id of ids) {
    const target = wanted.get(id);
    // Known at once (not when its turn comes), so an open page of the plugin waits for it.
    if (target && instances.get(id)?.key !== keyOf(target)) setState(id, { status: "loading" });
    const run = (queues.get(id) ?? Promise.resolve())
      .then(() => reconcile(id))
      .catch((error: unknown) => logPluginError(id, "the extension host failed", error));
    queues.set(id, run);
    runs.push(run);
  }
  return Promise.all(runs).then(() => undefined);
}

onRegistryReset(() => {
  instances.clear();
  wanted.clear();
  queues.clear();
  setStates({});
});

/** Drops every extension without calling into it (tests). */
export function resetExtensions() {
  resetRegistry();
}
