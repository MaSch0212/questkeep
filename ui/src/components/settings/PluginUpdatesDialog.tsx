import { createEffect, createMemo, createSignal, For, on, Show } from "solid-js";
import type { PluginChanges } from "../../bindings/PluginChanges";
import type { PluginInfo } from "../../bindings/PluginInfo";
import type { PluginUpdate } from "../../bindings/PluginUpdate";
import { translator } from "../../i18n";
import { pluginUpdates as pluginUpdatesMessages } from "../../i18n/pluginUpdates";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { pluginIconPath } from "../../lib/plugins";
import {
  askedFor,
  countOutcomes,
  hasExtension,
  type Prepared,
  planBatch,
  sortUpdates,
  type UpdateOutcome,
  versionChange,
} from "../../lib/pluginUpdates";
import { store } from "../../lib/store";
import { Modal } from "../Modal";
import { createRegistryInstall } from "../RegistryInstall";
import { PluginIcon } from "./PluginIcon";
import { scanAfterInstall } from "./scanPluginStores";

const tr = translator(pluginUpdatesMessages);

type Phase = "list" | "preparing" | "confirm" | "installing" | "done";

/** The installed plugin's icon as an image URL (null without an icon). */
function iconUrl(id: string): string | null {
  const plugin: PluginInfo | undefined = store.plugins().find((p) => p.id === id);
  const path = plugin ? pluginIconPath(plugin) : null;
  return path === null ? null : api.pluginFileUrl(id, path);
}

function outcome(
  update: PluginUpdate,
  status: UpdateOutcome["status"],
  message: string | null = null,
): UpdateOutcome {
  return {
    id: update.id,
    name: update.name,
    versions: versionChange(update),
    version: update.latestVersion,
    status,
    message,
  };
}

/**
 * The Plugin updates dialog: one row per plugin with a newer version (icon, name, versions,
 * release notes) and an Update button that opens the usual install dialog. "Update all" downloads
 * every update one after the other, installs those that ask for nothing new after one confirmation
 * and leaves the others in the list for the user to review.
 */
export function PluginUpdatesDialog(props: { onClose: () => void }) {
  const [phase, setPhase] = createSignal<Phase>("list");
  const [progress, setProgress] = createSignal({ done: 0, total: 0 });
  /** What the updates that need a look ask for, by plugin id. */
  const [flagged, setFlagged] = createSignal<Record<string, PluginChanges>>({});
  /** Downloaded and checked, waiting for the one confirmation. */
  const [ready, setReady] = createSignal<Prepared[]>([]);
  /** Outcomes known before the install step (downloads that failed, updates to review). */
  const [carried, setCarried] = createSignal<UpdateOutcome[]>([]);
  const [results, setResults] = createSignal<UpdateOutcome[]>([]);
  let cancelled = false;
  let root: HTMLDivElement | undefined;

  // Every step has its own buttons: the focus follows (the dialog itself while buttons are off,
  // so Esc keeps working).
  createEffect(
    on(
      phase,
      () =>
        queueMicrotask(() => {
          const target =
            root?.querySelector<HTMLElement>("[data-autofocus]:not([disabled])") ??
            root?.closest<HTMLElement>('[role="dialog"]');
          target?.focus();
        }),
      { defer: true },
    ),
  );

  // A single update from a row (the list or the results): the usual install dialog. What it
  // installed is no longer flagged, and its result row turns into "Updated".
  const registryInstall = createRegistryInstall((plugin) => {
    setFlagged(({ [plugin.id]: _done, ...rest }) => rest);
    setResults((current) =>
      current.map((item) =>
        item.id === plugin.id ? { ...item, status: "updated", message: null } : item,
      ),
    );
  });
  const updates = createMemo(() => sortUpdates(store.pluginUpdates()));
  const busy = () => phase() === "preparing" || phase() === "installing" || registryInstall.busy();

  function close() {
    // An install in progress is not interrupted: it ends with the results.
    if (phase() === "installing") return;
    cancelled = true;
    props.onClose();
  }

  async function updateAll() {
    if (busy()) return;
    const targets = updates();
    if (targets.length === 0) return;
    setResults([]);
    setPhase("preparing");
    const prepared: Prepared[] = [];
    const failed: UpdateOutcome[] = [];
    for (const [index, update] of targets.entries()) {
      if (cancelled) return;
      setProgress({ done: index + 1, total: targets.length });
      try {
        // oxlint-disable-next-line eslint/no-await-in-loop -- one download after the other, with progress
        const preview = await api.previewRegistryPlugin(update.id, update.latestVersion);
        prepared.push({ update, preview });
      } catch (error) {
        failed.push(outcome(update, "failed", errorMessage(error)));
      }
    }
    if (cancelled) return;
    const plan = planBatch(prepared);
    setFlagged(Object.fromEntries(plan.review.map((p) => [p.update.id, p.preview.changes])));
    const before = [
      ...failed,
      ...plan.review.map((p) => outcome(p.update, "review", askedFor(p.preview.changes))),
    ];
    if (plan.ready.length === 0) {
      setResults(before);
      setPhase(before.length > 0 ? "done" : "list");
      return;
    }
    setCarried(before);
    setReady(plan.ready);
    setPhase("confirm");
  }

  async function installReady() {
    const batch = ready();
    const outcomes = [...carried()];
    const installed: PluginInfo[] = [];
    setPhase("installing");
    for (const [index, item] of batch.entries()) {
      setProgress({ done: index + 1, total: batch.length });
      try {
        // No folders: the choices made earlier are kept.
        // oxlint-disable-next-line eslint/no-await-in-loop -- one install after the other
        installed.push(await api.installRegistryPlugin(item.update.id, item.update.latestVersion));
        outcomes.push(outcome(item.update, "updated"));
      } catch (error) {
        outcomes.push(outcome(item.update, "failed", errorMessage(error)));
      }
    }
    setReady([]);
    setResults(outcomes);
    setPhase("done");
    await store.loadPluginUpdates().catch(() => undefined);
    if (installed.length > 0)
      store.pushToast("info", tr("updatedToast", { count: installed.length }));
    for (const plugin of installed) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- one store scan after the other
      await scanAfterInstall(plugin);
    }
  }

  const resultText = (item: UpdateOutcome) => {
    switch (item.status) {
      case "updated":
        return tr("resultUpdated", { version: item.version });
      case "failed":
        return tr("resultFailed", { message: item.message ?? "" });
      case "review":
        return tr("resultReview", { what: item.message ?? "" });
    }
  };

  const confirmNames = () =>
    ready()
      .filter((p) => p.preview.preview.hasExtension)
      .map((p) => p.update.name)
      .join(", ");

  return (
    <Modal title={tr("title")} wide onClose={close}>
      <div class="plugin-updates" data-plugin-updates ref={(el) => (root = el)}>
        <Show when={phase() === "list" || phase() === "preparing"}>
          <p class="muted">{tr("intro")}</p>
          <div class="plugin-updates__head">
            <Show when={phase() === "preparing"}>
              <p class="plugin-updates__progress" role="status">
                {tr("downloading", { done: progress().done, total: progress().total })}
              </p>
            </Show>
            <button
              type="button"
              class="btn btn--primary plugin-updates__all"
              data-autofocus
              disabled={busy() || updates().length === 0}
              data-tooltip={tr("updateAllHint")}
              onClick={() => void updateAll()}
            >
              {tr("updateAll")}
            </button>
          </div>
          <Show when={updates().length > 0} fallback={<p class="muted">{tr("none")}</p>}>
            <ul class="plugin-updates__list" aria-label={tr("list")}>
              <For each={updates()}>
                {(update) => {
                  const changes = () => flagged()[update.id];
                  const preparing = () =>
                    registryInstall.preparing() === `${update.id}@${update.latestVersion}`;
                  return (
                    <li class="plugin-update" data-plugin-update-row={update.id}>
                      <PluginIcon src={iconUrl(update.id)} />
                      <div class="plugin-update__text">
                        <span class="account__name">
                          {update.name}{" "}
                          <span class="muted plugin__version">{versionChange(update)}</span>
                        </span>
                        <Show when={update.notes}>
                          {(notes) => (
                            <>
                              <span class="plugin__perm-title">{tr("notes")}</span>
                              <p class="plugin-update__notes plugin-prose">{notes()}</p>
                            </>
                          )}
                        </Show>
                        <Show when={changes()}>
                          {(found) => (
                            <p class="plugin-preview__replaces" data-plugin-needs-review>
                              {tr("reviewNeeded", { what: askedFor(found()) })}{" "}
                              <span data-plugin-review-hint>{tr("reviewHintList")}</span>
                            </p>
                          )}
                        </Show>
                      </div>
                      <button
                        type="button"
                        class="btn"
                        data-plugin-update={update.id}
                        disabled={busy()}
                        aria-busy={preparing()}
                        aria-label={tr("updateLabel", {
                          name: update.name,
                          version: update.latestVersion,
                        })}
                        onClick={() => void registryInstall.begin(update.id, update.latestVersion)}
                      >
                        {tr("update")}
                      </button>
                    </li>
                  );
                }}
              </For>
            </ul>
          </Show>
          <div class="form__actions">
            <button type="button" class="btn" onClick={close}>
              {tr("close")}
            </button>
          </div>
        </Show>

        <Show when={phase() === "confirm"}>
          <h3 class="plugin-updates__heading">{tr("confirmTitle", { count: ready().length })}</h3>
          <p class="muted">{tr("confirmIntro")}</p>
          <ul class="plugin-updates__list" aria-label={tr("list")}>
            <For each={ready()}>
              {(item) => (
                <li class="plugin-update" data-plugin-confirm-row={item.update.id}>
                  <PluginIcon src={iconUrl(item.update.id)} />
                  <div class="plugin-update__text">
                    <span class="account__name">
                      {item.update.name}{" "}
                      <span class="muted plugin__version">{versionChange(item.update)}</span>
                    </span>
                    <Show when={item.update.notes}>
                      {(notes) => <p class="plugin-update__notes plugin-prose">{notes()}</p>}
                    </Show>
                  </div>
                </li>
              )}
            </For>
          </ul>
          <Show when={hasExtension(ready())}>
            <p class="muted plugin-preview__trust" data-plugin-extension-warning>
              {tr("confirmExtension", { names: confirmNames() })}
            </p>
          </Show>
          <div class="form__actions">
            <button
              type="button"
              class="btn"
              data-autofocus
              onClick={() => {
                // Back to the list: what was found stays flagged.
                setReady([]);
                setPhase("list");
              }}
            >
              {tr("back")}
            </button>
            <button type="button" class="btn btn--primary" onClick={() => void installReady()}>
              {tr("confirmTitle", { count: ready().length })}
            </button>
          </div>
        </Show>

        <Show when={phase() === "installing"}>
          <p class="plugin-updates__progress" role="status">
            {tr("installing", { done: progress().done, total: progress().total })}
          </p>
        </Show>

        <Show when={phase() === "done"}>
          <h3 class="plugin-updates__heading">{tr("results")}</h3>
          <p class="muted" role="status">
            {tr("summary", {
              updated: countOutcomes(results()).updated,
              failed: countOutcomes(results()).failed,
              review: countOutcomes(results()).review,
            })}
          </p>
          <ul class="plugin-updates__list" aria-label={tr("results")}>
            <For each={results()}>
              {(item) => (
                <li class="plugin-update" data-plugin-result={item.id}>
                  <PluginIcon src={iconUrl(item.id)} />
                  <div class="plugin-update__text">
                    <span class="account__name">
                      {item.name} <span class="muted plugin__version">{item.versions}</span>
                    </span>
                    <span
                      class="plugin-update__result selectable"
                      classList={{
                        "plugin-update__result--failed": item.status === "failed",
                        "plugin-update__result--review": item.status === "review",
                      }}
                    >
                      {resultText(item)}
                    </span>
                    <Show when={item.status === "review"}>
                      <span class="muted" data-plugin-review-hint>
                        {tr("reviewHintResult")}
                      </span>
                    </Show>
                  </div>
                  <Show when={item.status === "review"}>
                    <button
                      type="button"
                      class="btn"
                      data-plugin-update={item.id}
                      disabled={busy()}
                      aria-busy={registryInstall.preparing() === `${item.id}@${item.version}`}
                      aria-label={tr("updateLabel", { name: item.name, version: item.version })}
                      onClick={() => void registryInstall.begin(item.id, item.version)}
                    >
                      {tr("update")}
                    </button>
                  </Show>
                </li>
              )}
            </For>
          </ul>
          <div class="form__actions">
            <Show when={updates().length > 0}>
              <button type="button" class="btn" onClick={() => setPhase("list")}>
                {tr("back")}
              </button>
            </Show>
            <button type="button" class="btn btn--primary" data-autofocus onClick={close}>
              {tr("close")}
            </button>
          </div>
        </Show>
      </div>
      {registryInstall.dialog()}
    </Modal>
  );
}
