import ArchiveRestore from "lucide-solid/icons/archive-arrow-up";
import DatabaseBackup from "lucide-solid/icons/database-backup";
import { createEffect, createSignal, For, on, Show } from "solid-js";
import type { BackupFrequency } from "../../bindings/BackupFrequency";
import type { BackupInfo } from "../../bindings/BackupInfo";
import type { BackupSettings } from "../../bindings/BackupSettings";
import { translator } from "../../i18n";
import { settingsBackups } from "../../i18n/settingsBackups";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { formatAgo, formatDateTime, formatSize } from "../../lib/format";
import { pickDirectory } from "../../lib/pickers";
import { store } from "../../lib/store";
import { ConfirmDialog } from "../ConfirmDialog";
import type { SettingsModel } from "./useSettings";

const tr = translator(settingsBackups);

const FREQUENCIES: { value: BackupFrequency; label: "everyStart" | "daily" | "weekly" }[] = [
  { value: "everyStart", label: "everyStart" },
  { value: "daily", label: "daily" },
  { value: "weekly", label: "weekly" },
];

/** Archives kept: at least one, and no more than the field allows. */
const MAX_KEEP = 50;

async function backUpNow() {
  try {
    await api.backupNow();
    store.pushToast("info", tr("written"));
  } catch {
    // The failure arrives as a core event and shows as a toast.
  }
}

export function BackupsSection(props: { model: SettingsModel }) {
  const [backups, setBackups] = createSignal<BackupInfo[]>([]);
  const [folder, setFolder] = createSignal("");
  const [restoring, setRestoring] = createSignal<BackupInfo | null>(null);
  const [restartPending, setRestartPending] = createSignal(false);

  async function load() {
    try {
      const [list, dir] = await Promise.all([api.listBackups(), api.backupFolder()]);
      setBackups(list ?? []);
      setFolder(dir ?? "");
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    }
  }

  // Loads on open and again after every finished backup.
  createEffect(on(store.backupsChanged, () => void load()));

  function change(patch: Partial<BackupSettings>) {
    const current = props.model.settings()?.backups;
    if (!current) return;
    void props.model.update({ backups: { ...current, ...patch } }).then(load);
  }

  async function chooseFolder() {
    const dir = await pickDirectory();
    if (dir !== null) change({ folder: dir });
  }

  async function restore(backup: BackupInfo) {
    setRestartPending(true);
    try {
      await api.restoreBackup(backup.path);
    } catch (error) {
      setRestartPending(false);
      store.pushToast("error", errorMessage(error));
    }
  }

  return (
    <Show when={props.model.settings()?.backups} fallback={<p class="muted">{tr("loading")}</p>}>
      {(current) => (
        <div class="form">
          <label class="check">
            <input
              type="checkbox"
              checked={current().enabled}
              onChange={(e) => change({ enabled: e.currentTarget.checked })}
            />
            <span>{tr("auto")}</span>
          </label>
          <div class="settings-row" classList={{ "check--disabled": !current().enabled }}>
            <label for="backup-frequency">{tr("howOften")}</label>
            <select
              id="backup-frequency"
              class="input settings-row__input"
              value={current().frequency}
              disabled={!current().enabled}
              onChange={(e) => change({ frequency: e.currentTarget.value as BackupFrequency })}
            >
              <For each={FREQUENCIES}>{(f) => <option value={f.value}>{tr(f.label)}</option>}</For>
            </select>
          </div>
          <div class="settings-row">
            <label for="backup-keep">{tr("keep")}</label>
            <input
              id="backup-keep"
              class="input settings-row__number"
              type="number"
              min="1"
              max={MAX_KEEP}
              value={current().keep}
              onChange={(e) => {
                const keep = Math.round(Number(e.currentTarget.value));
                if (Number.isFinite(keep) && keep >= 1 && keep <= MAX_KEEP) change({ keep });
                else e.currentTarget.value = String(current().keep);
              }}
            />
          </div>
          <div class="settings-row">
            <span id="backup-folder-label">{tr("folder")}</span>
            <span
              class="backup-folder"
              aria-labelledby="backup-folder-label"
              data-tooltip={folder()}
            >
              {folder()}
            </span>
            <button type="button" class="btn" onClick={() => void chooseFolder()}>
              {tr("change")}
            </button>
            <Show when={current().folder !== null}>
              <button type="button" class="btn" onClick={() => change({ folder: null })}>
                {tr("useDefault")}
              </button>
            </Show>
          </div>
          <p class="field__hint">{tr("hint")}</p>
          <div class="scan-row">
            <button
              type="button"
              class="btn"
              disabled={store.backupRunning()}
              onClick={() => void backUpNow()}
            >
              <DatabaseBackup class="icon" aria-hidden="true" /> {tr("backUpNow")}
            </button>
            <Show when={!store.backupRunning() && backups()[0]}>
              {(last) => (
                <span class="muted">
                  {tr("lastBackup", {
                    ago: formatAgo(last().createdAt),
                    size: formatSize(last().size),
                  })}
                </span>
              )}
            </Show>
            <Show when={store.backupRunning()}>
              <span role="status">{store.backgroundWorkLabel()}</span>
              <button type="button" class="btn" onClick={() => void api.cancelBackup()}>
                {tr("cancel")}
              </button>
            </Show>
          </div>
          <Show when={backups().length > 0} fallback={<p class="muted">{tr("none")}</p>}>
            <ul class="backup-list" aria-label={tr("list")}>
              <For each={backups()}>
                {(backup) => (
                  <li class="backup-list__item">
                    <span class="backup-list__date" data-tooltip={formatDateTime(backup.createdAt)}>
                      {formatAgo(backup.createdAt)}
                    </span>
                    <span class="muted">
                      {formatSize(backup.size)}
                      <Show when={backup.appVersion}>{(v) => ` · Questkeep ${v()}`}</Show>
                    </span>
                    <button
                      type="button"
                      class="btn btn--small"
                      disabled={store.backupRunning() || restartPending()}
                      aria-label={tr("restoreAria", { date: formatDateTime(backup.createdAt) })}
                      onClick={() => setRestoring(backup)}
                    >
                      <ArchiveRestore class="icon" aria-hidden="true" /> {tr("restore")}
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
          <Show when={restartPending()}>
            <p role="status">{tr("restarting")}</p>
          </Show>
          <Show when={restoring()} keyed>
            {(backup) => (
              <ConfirmDialog
                title={tr("restoreTitle")}
                message={tr("restoreMessage", { date: formatDateTime(backup.createdAt) })}
                confirmLabel={tr("restoreConfirm")}
                onConfirm={() => void restore(backup)}
                onClose={() => setRestoring(null)}
              />
            )}
          </Show>
        </div>
      )}
    </Show>
  );
}
