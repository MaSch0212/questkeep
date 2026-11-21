import { open as openDialog } from "@tauri-apps/plugin-dialog";
import ChevronRight from "lucide-solid/icons/chevron-right";
import { createSignal, For, type JSX, Show } from "solid-js";
import type { PluginFolder } from "../../bindings/PluginFolder";
import type { PluginInfo } from "../../bindings/PluginInfo";
import type { PluginPreview } from "../../bindings/PluginPreview";
import { translator } from "../../i18n";
import { extensions } from "../../i18n/extensions";
import { pluginBrowser } from "../../i18n/pluginBrowser";
import { settingsPlugins } from "../../i18n/settingsPlugins";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { extensionStates } from "../../lib/extensions/host";
import { pluginThemeProblems } from "../../lib/pluginThemes";
import {
  hasPermissions,
  pluginIconPath,
  pluginKinds,
  pluginStatus,
  sortPlugins,
} from "../../lib/plugins";
import { store } from "../../lib/store";
import { ConfirmDialog } from "../ConfirmDialog";
import { createRegistryInstall } from "../RegistryInstall";
import { PluginBrowser } from "./PluginBrowser";
import { PluginFolders } from "./PluginFolders";
import { PluginIcon } from "./PluginIcon";
import { PluginInstallDialog } from "./PluginInstallDialog";
import { pickPluginFolder } from "./pluginFolderPicker";
import { scanAfterInstall, scanPluginStores } from "./scanPluginStores";

const tr = translator(settingsPlugins);
const trExtensions = translator(extensions);
const trRegistry = translator(pluginBrowser);

const KIND_LABEL = {
  store: "kindStore",
  extension: "kindExtension",
  theme: "kindTheme",
} as const;
const STATUS_LABEL = {
  loaded: "statusLoaded",
  disabled: "statusDisabled",
  failed: "statusFailed",
} as const;

/** A plugin folder or .zip file from the system dialog; null when cancelled. */
async function pickPlugin(kind: "folder" | "file"): Promise<string | null> {
  const result =
    kind === "folder"
      ? await openDialog({ multiple: false, directory: true, title: tr("chooseFolder") })
      : await openDialog({
          multiple: false,
          directory: false,
          title: tr("chooseFile"),
          filters: [{ name: tr("filterPlugin"), extensions: ["zip"] }],
        });
  return typeof result === "string" ? result : null;
}

const plugins = () => sortPlugins(store.plugins());

/** Install, reload, disable and remove plugins. The list follows the `pluginsChanged` event. */
export function PluginsSection(props: { updateCheck?: JSX.Element }) {
  /** A chosen plugin, read and checked, waiting for the go-ahead. */
  const [pending, setPending] = createSignal<{ path: string; plugin: PluginPreview } | null>(null);
  const [installing, setInstalling] = createSignal(false);
  const [removing, setRemoving] = createSignal<PluginInfo | null>(null);
  const [deleteData, setDeleteData] = createSignal(false);
  /** Plugin ids with an action in flight. */
  const [busy, setBusy] = createSignal<ReadonlySet<string>>(new Set());
  /** The last failure per plugin, shown in its row until the next successful action. */
  const [errors, setErrors] = createSignal<Record<string, string>>({});
  const [open, setOpen] = createSignal<ReadonlySet<string>>(new Set());
  const [browsing, setBrowsing] = createSignal(false);
  /** Updating a registry plugin from its row: the same download, check and confirm as the browser. */
  const registryInstall = createRegistryInstall();

  function setError(id: string, message: string | null) {
    setErrors(({ [id]: _old, ...rest }) => (message === null ? rest : { ...rest, [id]: message }));
  }

  function toggleOpen(id: string) {
    setOpen((current) => {
      const next = new Set(current);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  }

  async function choose(kind: "folder" | "file") {
    try {
      const path = await pickPlugin(kind);
      // A folder or file that is no plugin is refused here, with the reason.
      if (path !== null) setPending({ path, plugin: await api.previewPlugin(path) });
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    }
  }

  async function install(path: string, folders?: Record<string, string>) {
    setInstalling(true);
    try {
      const plugin = await (folders ? api.installPlugin(path, folders) : api.installPlugin(path));
      store.pushToast("info", tr("installed", { name: plugin.name }));
      setError(plugin.id, null);
      await scanAfterInstall(plugin);
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      setInstalling(false);
    }
  }

  /** Runs one action on a plugin: its buttons stay off meanwhile, a failure stays in its row. */
  async function act(
    plugin: PluginInfo,
    run: () => Promise<PluginInfo>,
    done: (updated: PluginInfo) => string | null,
    rescan: boolean,
  ) {
    if (busy().has(plugin.id)) return;
    setBusy((current) => new Set(current).add(plugin.id));
    try {
      const updated = await run();
      setError(plugin.id, null);
      const message = done(updated);
      if (message !== null) store.pushToast("info", message);
      if (rescan) await scanPluginStores(updated);
    } catch (error) {
      const message = errorMessage(error);
      setError(plugin.id, message);
      store.pushToast("error", message);
    } finally {
      setBusy((current) => {
        const next = new Set(current);
        next.delete(plugin.id);
        return next;
      });
    }
  }

  const reload = (plugin: PluginInfo) =>
    act(
      plugin,
      () => api.reloadPlugin(plugin.id),
      (updated) => tr("reloaded", { name: updated.name }),
      true,
    );

  const setEnabled = (plugin: PluginInfo, enabled: boolean) =>
    act(
      plugin,
      () => api.setPluginEnabled(plugin.id, enabled),
      (updated) => tr(enabled ? "enabledToast" : "disabledToast", { name: updated.name }),
      enabled,
    );

  /** The user picks a folder for one the plugin asks for; its stores load again and scan. */
  async function chooseFolder(plugin: PluginInfo, folder: PluginFolder) {
    let path: string | null;
    try {
      path = await pickPluginFolder(folder.label, folder.exists ? folder.effectivePath : null);
    } catch (error) {
      store.pushToast("error", errorMessage(error));
      return;
    }
    if (path === null) return;
    const chosen = path;
    await act(
      plugin,
      () => api.setPluginFolder(plugin.id, folder.key, chosen),
      (updated) =>
        tr("folderSet", {
          name: updated.name,
          label: folder.label,
          path: updated.folders.find((f) => f.key === folder.key)?.path ?? chosen,
        }),
      true,
    );
  }

  const useDefaultFolder = (plugin: PluginInfo, folder: PluginFolder) =>
    act(
      plugin,
      () => api.setPluginFolder(plugin.id, folder.key, null),
      (updated) => tr("folderReset", { name: updated.name, label: folder.label }),
      true,
    );

  const setWatch = (plugin: PluginInfo, watch: boolean) =>
    act(
      plugin,
      () => api.setPluginWatch(plugin.id, watch),
      () => null,
      false,
    );

  async function remove(plugin: PluginInfo, withData: boolean) {
    if (busy().has(plugin.id)) return;
    setBusy((current) => new Set(current).add(plugin.id));
    try {
      await api.removePlugin(plugin.id, withData);
      setError(plugin.id, null);
      store.pushToast("info", tr("removed", { name: plugin.name }));
    } catch (error) {
      const message = errorMessage(error);
      setError(plugin.id, message);
      store.pushToast("error", message);
    } finally {
      setBusy((current) => {
        const next = new Set(current);
        next.delete(plugin.id);
        return next;
      });
    }
  }

  return (
    <div class="form">
      <p class="muted">{tr("intro")}</p>
      <p class="muted">
        {tr("themesHint")}{" "}
        <button
          type="button"
          class="link-btn"
          onClick={() => store.showSettingsSection("appearance")}
        >
          {tr("openAppearance")}
        </button>
      </p>
      {props.updateCheck}
      <div class="plugin-install">
        <button
          type="button"
          class="btn"
          disabled={installing()}
          onClick={() => void choose("folder")}
        >
          {tr("installFolder")}
        </button>
        <button
          type="button"
          class="btn"
          disabled={installing()}
          onClick={() => void choose("file")}
        >
          {tr("installFile")}
        </button>
        <button type="button" class="btn" data-plugin-browse onClick={() => setBrowsing(true)}>
          {trRegistry("browse")}
          <Show when={store.pluginUpdates().length > 0}>
            <span
              class="account__badge plugin-card__update"
              role="status"
              aria-label={trRegistry("browseUpdates", { count: store.pluginUpdates().length })}
              data-tooltip={trRegistry("browseUpdates", { count: store.pluginUpdates().length })}
            >
              {store.pluginUpdates().length}
            </span>
          </Show>
        </button>
      </div>
      <Show when={plugins().length > 0} fallback={<p class="muted">{tr("empty")}</p>}>
        <div class="account-list" role="list" aria-label={tr("list")}>
          <For each={plugins()}>
            {(plugin) => {
              const status = () => pluginStatus(plugin);
              const working = () => busy().has(plugin.id);
              const expanded = () => open().has(plugin.id);
              const error = () => errors()[plugin.id] ?? null;
              const update = () => store.pluginUpdates().find((u) => u.id === plugin.id);
              const iconUrl = () => {
                const path = pluginIconPath(plugin);
                return path === null ? null : api.pluginFileUrl(plugin.id, path);
              };
              /** Why the plugin's extension did not start, if it did not. */
              const extensionError = () => {
                const state = extensionStates()[plugin.id];
                return state?.status === "failed" ? (state.error ?? "") : null;
              };
              return (
                <div
                  class="account plugin"
                  classList={{ "plugin--failed": status() === "failed" }}
                  role="group"
                  aria-label={plugin.name}
                >
                  <div class="account__head">
                    <PluginIcon src={iconUrl()} />
                    <div class="account__info">
                      <span class="account__name">
                        {plugin.name} <span class="muted plugin__version">{plugin.version}</span>
                      </span>
                      <Show when={plugin.registry}>
                        {(registry) => (
                          <span class="muted account__detail">
                            {trRegistry("fromRegistry", { version: registry().version })}
                          </span>
                        )}
                      </Show>
                      <Show when={plugin.author}>
                        {(author) => (
                          <span class="muted account__detail">
                            {tr("by", { author: author() })}
                          </span>
                        )}
                      </Show>
                    </div>
                    <For each={pluginKinds(plugin)}>
                      {(kind) => <span class="account__badge">{tr(KIND_LABEL[kind])}</span>}
                    </For>
                    <Show when={plugin.registry?.pinned}>
                      <span class="account__badge">{trRegistry("pinned")}</span>
                    </Show>
                    <span
                      class="account__badge"
                      classList={{
                        "account__badge--on": status() === "loaded",
                        "account__badge--danger": status() === "failed",
                      }}
                      role="status"
                    >
                      {tr(STATUS_LABEL[status()])}
                    </span>
                  </div>
                  <Show when={plugin.description}>
                    {(description) => <p class="plugin__description">{description()}</p>}
                  </Show>
                  <Show when={plugin.error}>
                    {(message) => <p class="plugin__error selectable">{message()}</p>}
                  </Show>
                  <Show when={extensionError() !== null}>
                    <p class="plugin__error selectable" role="alert">
                      {trExtensions("extensionFailed", { error: extensionError() ?? "" })}
                    </p>
                  </Show>
                  <For each={pluginThemeProblems()[plugin.id] ?? []}>
                    {(problem) => (
                      <p class="plugin__error selectable" role="alert">
                        {tr("themeFailed", { name: problem.name, error: problem.error })}
                      </p>
                    )}
                  </For>
                  <Show when={error() !== null && error() !== plugin.error}>
                    <p class="plugin__error selectable" role="alert">
                      {error()}
                    </p>
                  </Show>
                  <Show when={plugin.folders.length > 0}>
                    <PluginFolders
                      plugin={plugin}
                      disabled={working()}
                      onChoose={(folder) => void chooseFolder(plugin, folder)}
                      onUseDefault={(folder) => void useDefaultFolder(plugin, folder)}
                    />
                  </Show>
                  <button
                    type="button"
                    class="link-btn account__disclosure"
                    aria-label={tr("detailsOf", { name: plugin.name })}
                    aria-expanded={expanded()}
                    aria-controls={`plugin-${plugin.id}-details`}
                    onClick={() => toggleOpen(plugin.id)}
                  >
                    <ChevronRight
                      class="icon account__chevron"
                      classList={{ "account__chevron--open": expanded() }}
                      aria-hidden="true"
                    />{" "}
                    {tr("details")}
                  </button>
                  <Show when={expanded()}>
                    <dl class="plugin__details" id={`plugin-${plugin.id}-details`}>
                      <dt>{tr("source")}</dt>
                      <dd class="selectable">
                        {plugin.source}
                        <Show when={!plugin.sourceExists}>
                          <span class="field__hint plugin__hint">{tr("sourceGone")}</span>
                        </Show>
                      </dd>
                      <dt>{tr("stores")}</dt>
                      <dd class="selectable">
                        {plugin.providers.length > 0 ? plugin.providers.join(", ") : tr("noStores")}
                      </dd>
                      <dt>{tr("permissions")}</dt>
                      <dd>
                        <Show
                          when={hasPermissions(plugin)}
                          fallback={<span class="muted">{tr("noPermissions")}</span>}
                        >
                          <Show when={plugin.permissions.filesystem || plugin.permissions.registry}>
                            <ul class="plugin__perms selectable">
                              <Show when={plugin.permissions.filesystem}>
                                <li>{tr("permFilesystem")}</li>
                              </Show>
                              <Show when={plugin.permissions.registry}>
                                <li>{tr("permRegistry")}</li>
                              </Show>
                            </ul>
                          </Show>
                          <Show when={plugin.permissions.read.length > 0}>
                            <span class="plugin__perm-title">{tr("readFolders")}</span>
                            <ul class="plugin__perms selectable">
                              <For each={plugin.permissions.read}>{(path) => <li>{path}</li>}</For>
                            </ul>
                          </Show>
                          <Show when={plugin.permissions.folders.length > 0}>
                            <ul class="plugin__perms selectable">
                              <For each={plugin.permissions.folders}>
                                {(folder) => (
                                  <li>{tr("folderRequest", { label: folder.label })}</li>
                                )}
                              </For>
                            </ul>
                          </Show>
                          <Show when={plugin.permissions.http.length > 0}>
                            <span class="plugin__perm-title">{tr("httpHosts")}</span>
                            <ul class="plugin__perms selectable">
                              <For each={plugin.permissions.http}>{(host) => <li>{host}</li>}</For>
                            </ul>
                          </Show>
                          <Show when={plugin.permissions.login.length > 0}>
                            <span class="plugin__perm-title">{tr("permLogin")}</span>
                            <ul class="plugin__perms selectable">
                              <For each={plugin.permissions.login}>
                                {(domain) => <li>{domain}</li>}
                              </For>
                            </ul>
                          </Show>
                          <Show when={plugin.permissions.launcher.length > 0}>
                            <span class="plugin__perm-title">{tr("permLauncher")}</span>
                            <ul class="plugin__perms selectable">
                              <For each={plugin.permissions.launcher}>
                                {(name) => <li>{name}</li>}
                              </For>
                            </ul>
                          </Show>
                        </Show>
                      </dd>
                    </dl>
                  </Show>
                  <div class="plugin__actions">
                    <Show when={update()}>
                      {(found) => (
                        <button
                          type="button"
                          class="btn btn--primary"
                          data-plugin-update={plugin.id}
                          disabled={working() || registryInstall.busy()}
                          aria-busy={
                            registryInstall.preparing() === `${plugin.id}@${found().latestVersion}`
                          }
                          onClick={() =>
                            void registryInstall.begin(plugin.id, found().latestVersion)
                          }
                        >
                          {registryInstall.preparing() === `${plugin.id}@${found().latestVersion}`
                            ? trRegistry("downloading")
                            : trRegistry("updateTo", { version: found().latestVersion })}
                        </button>
                      )}
                    </Show>
                    <label
                      class="check check--switch"
                      classList={{ "check--disabled": !plugin.sourceExists }}
                      data-tooltip={plugin.sourceExists ? tr("watchHint") : tr("watchGone")}
                    >
                      <input
                        type="checkbox"
                        role="switch"
                        checked={plugin.watch}
                        disabled={!plugin.sourceExists || working()}
                        onChange={(event) => void setWatch(plugin, event.currentTarget.checked)}
                      />
                      {tr("watch")}
                    </label>
                    <button
                      type="button"
                      class="btn"
                      disabled={!plugin.enabled || working()}
                      data-tooltip={plugin.enabled ? tr("reloadHint") : tr("reloadDisabled")}
                      onClick={() => void reload(plugin)}
                    >
                      {tr("reload")}
                    </button>
                    <button
                      type="button"
                      class="btn"
                      disabled={working()}
                      onClick={() => void setEnabled(plugin, !plugin.enabled)}
                    >
                      {plugin.enabled ? tr("disable") : tr("enable")}
                    </button>
                    <button
                      type="button"
                      class="btn btn--danger"
                      disabled={working()}
                      onClick={() => {
                        setDeleteData(false);
                        setRemoving(plugin);
                      }}
                    >
                      {tr("remove")}
                    </button>
                  </div>
                </div>
              );
            }}
          </For>
        </div>
      </Show>
      <Show when={pending()}>
        {(chosen) => {
          // The dialog closes before it confirms, which ends the <Show>: keep the value.
          const { path, plugin } = chosen();
          return (
            <PluginInstallDialog
              path={path}
              plugin={plugin}
              onInstall={(folders) => void install(path, folders)}
              onClose={() => setPending(null)}
            />
          );
        }}
      </Show>
      <Show when={browsing()}>
        <PluginBrowser onClose={() => setBrowsing(false)} />
      </Show>
      {registryInstall.dialog()}
      <Show when={removing()}>
        {(plugin) => {
          const target = plugin();
          return (
            <ConfirmDialog
              title={tr("removeTitle", { name: target.name })}
              message={
                <>
                  {tr("removeMessage")}
                  <label class="check plugin__delete-data">
                    <input
                      type="checkbox"
                      checked={deleteData()}
                      onChange={(event) => setDeleteData(event.currentTarget.checked)}
                    />
                    {tr("deleteData")}
                  </label>
                </>
              }
              confirmLabel={tr("removeConfirm")}
              onConfirm={() => void remove(target, deleteData())}
              onClose={() => setRemoving(null)}
            />
          );
        }}
      </Show>
    </div>
  );
}
