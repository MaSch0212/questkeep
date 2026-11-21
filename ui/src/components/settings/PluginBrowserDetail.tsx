import ArrowLeft from "lucide-solid/icons/arrow-left";
import { createSignal, For, onMount, Show } from "solid-js";
import type { RegistryPluginView } from "../../bindings/RegistryPluginView";
import type { RegistryVersionView } from "../../bindings/RegistryVersionView";
import { translator } from "../../i18n";
import { pluginBrowser } from "../../i18n/pluginBrowser";
import { settingsPlugins } from "../../i18n/settingsPlugins";
import { formatReleaseDate, formatSize } from "../../lib/format";
import {
  hasHiddenVersions,
  listedVersions,
  registryKinds,
  versionAction,
  versionBlock,
} from "../../lib/pluginRegistry";
import { openLink } from "./openLink";
import { PluginIcon } from "./PluginIcon";

const tr = translator(pluginBrowser);
const trPlugins = translator(settingsPlugins);

const KIND_LABEL = {
  store: "kindStore",
  extension: "kindExtension",
  theme: "kindTheme",
} as const;

/** One plugin of the registry: what it is, and its versions with an Install/Update button each. */
export function PluginBrowserDetail(props: {
  plugin: RegistryPluginView;
  /** `id@version` while that version is downloaded and checked. */
  preparing: string | null;
  /** No other install may start now. */
  busy: boolean;
  onBack: () => void;
  onInstall: (version: string) => void;
}) {
  const [showAll, setShowAll] = createSignal(false);
  const plugin = () => props.plugin;
  const versions = () => listedVersions(plugin(), showAll());
  const [back, setBack] = createSignal<HTMLButtonElement>();
  onMount(() => back()?.focus());

  const reason = (version: RegistryVersionView) =>
    versionBlock(version) === "yanked"
      ? tr("yanked")
      : tr("incompatibleVersion", { minApp: version.minApp, api: version.apiVersion });

  return (
    <div class="plugin-detail">
      <button
        ref={setBack}
        type="button"
        class="btn btn--ghost plugin-detail__back"
        onClick={props.onBack}
      >
        <ArrowLeft class="icon" aria-hidden="true" />
        {tr("back")}
      </button>
      <header class="plugin-detail__head">
        <PluginIcon src={plugin().iconData} large />
        <div class="plugin-detail__title">
          <h3 class="plugin-detail__name">{plugin().name}</h3>
          <span class="muted">{trPlugins("by", { author: plugin().author })}</span>
        </div>
        <div class="plugin-card__badges">
          <For each={registryKinds(plugin())}>
            {(kind) => <span class="account__badge">{trPlugins(KIND_LABEL[kind])}</span>}
          </For>
          <Show when={plugin().installed}>
            {(installed) => (
              <span class="account__badge account__badge--on">
                {tr("installedVersion", { version: installed().version })}
              </span>
            )}
          </Show>
          <Show when={plugin().updateAvailable}>
            <span class="account__badge plugin-card__update">{tr("updateAvailable")}</span>
          </Show>
        </div>
      </header>
      <p class="plugin__description plugin-prose">{plugin().description}</p>
      <dl class="plugin__details">
        <dt>{tr("license")}</dt>
        <dd class="selectable">{plugin().license}</dd>
        <dt>{tr("repository")}</dt>
        <dd>
          <button type="button" class="link-btn" onClick={() => void openLink(plugin().repository)}>
            {plugin().repository}
          </button>
        </dd>
        <Show when={plugin().homepage}>
          {(homepage) => (
            <>
              <dt>{tr("homepage")}</dt>
              <dd>
                <button type="button" class="link-btn" onClick={() => void openLink(homepage())}>
                  {homepage()}
                </button>
              </dd>
            </>
          )}
        </Show>
      </dl>
      <Show when={plugin().latest === null}>
        <p class="plugin__error" role="alert">
          {tr("notCompatible", { reason: plugin().incompatibleReason ?? "" })}
        </p>
      </Show>
      <Show when={plugin().installed}>
        {(installed) => (
          <>
            <Show when={installed().pinned}>
              <p class="plugin-preview__replaces">
                {tr("pinnedNote", { version: installed().version })}
              </p>
            </Show>
            <Show when={!installed().fromRegistry}>
              <p class="plugin-preview__replaces">{tr("folderNote")}</p>
            </Show>
          </>
        )}
      </Show>
      <div class="plugin-detail__versions-head">
        <h3 class="plugin-detail__heading">{tr("versions")}</h3>
        <Show when={hasHiddenVersions(plugin())}>
          <label class="check">
            <input
              type="checkbox"
              checked={showAll()}
              onChange={(event) => setShowAll(event.currentTarget.checked)}
            />
            {tr("showAll")}
          </label>
        </Show>
      </div>
      <Show
        when={versions().length > 0}
        fallback={
          <>
            <p class="muted">{tr("noVersions")}</p>
            <button type="button" class="btn btn--primary" disabled>
              {tr("install")}
            </button>
          </>
        }
      >
        <ul class="plugin-versions" aria-label={tr("versions")}>
          <For each={versions()}>
            {(version) => {
              const block = () => versionBlock(version);
              const action = () => versionAction(plugin(), version.version);
              const key = () => `${plugin().id}@${version.version}`;
              const isCurrent = () =>
                plugin().installed?.fromRegistry === true &&
                plugin().installed?.version === version.version;
              return (
                <li
                  class="plugin-version"
                  classList={{ "plugin-version--blocked": block() !== null }}
                >
                  <div class="plugin-version__info">
                    <span class="plugin-version__title">
                      <span class="account__name">v{version.version}</span>
                      <Show when={isCurrent()}>
                        <span class="account__badge account__badge--on">{tr("current")}</span>
                      </Show>
                      <Show when={version.version === plugin().latest}>
                        <span class="account__badge">{tr("newest")}</span>
                      </Show>
                    </span>
                    <span class="muted plugin-version__meta">
                      {version.released
                        ? tr("releasedOn", { date: formatReleaseDate(version.released) ?? "" })
                        : ""}
                      {version.released ? " · " : ""}
                      {formatSize(version.size)}
                    </span>
                    <Show when={version.notes}>
                      {(notes) => <span class="plugin-version__notes plugin-prose">{notes()}</span>}
                    </Show>
                    <Show when={block() !== null}>
                      <span class="plugin-card__note">{reason(version)}</span>
                    </Show>
                  </div>
                  <button
                    type="button"
                    class="btn btn--small"
                    classList={{
                      "btn--primary":
                        block() === null &&
                        version.version === plugin().latest &&
                        action() !== "reinstall",
                    }}
                    disabled={block() !== null || props.busy}
                    aria-busy={props.preparing === key()}
                    aria-label={tr("actionLabel", {
                      action: tr(action()),
                      name: plugin().name,
                      version: version.version,
                    })}
                    onClick={() => props.onInstall(version.version)}
                  >
                    {props.preparing === key() ? tr("downloading") : tr(action())}
                  </button>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>
    </div>
  );
}
