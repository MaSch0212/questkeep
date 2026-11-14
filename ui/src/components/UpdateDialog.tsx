import Download from "lucide-solid/icons/download";
import ExternalLink from "lucide-solid/icons/external-link";
import { createSignal, Show } from "solid-js";
import type { UpdateInfo } from "../lib/api";
import * as api from "../lib/api";
import { translator } from "../i18n";
import { shellDialogs } from "../i18n/shellDialogs";
import { errorMessage } from "../lib/errors";
import { formatSize } from "../lib/format";
import { store } from "../lib/store";
import { Markdown } from "./Markdown";
import { Modal } from "./Modal";

const tr = translator(shellDialogs);

/** How far the update download is, in percent; null while the size is unknown. */
function percent(): number | null {
  const progress = store.updateProgress();
  if (!progress?.total) return null;
  return Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
}

function progressText(): string {
  const progress = store.updateProgress();
  if (progress?.installing) return tr("installingUpdate");
  if (!progress) return tr("startingDownload");
  return progress.total
    ? tr("downloadProgress", {
        done: formatSize(progress.downloaded),
        total: formatSize(progress.total),
      })
    : tr("downloadedSoFar", { done: formatSize(progress.downloaded) });
}

/** Offers a newer version: its release notes, then install it now (with download progress; the
 * app restarts where it was) or later. A downloaded copy links to the release page instead. */
export function UpdateDialog(props: { update: UpdateInfo; onClose: () => void }) {
  const [installing, setInstalling] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  async function install() {
    setError(null);
    if (props.update.manual) {
      try {
        await api.installUpdate();
        props.onClose();
      } catch (failure) {
        setError(errorMessage(failure));
      }
      return;
    }
    setInstalling(true);
    store.setUpdateProgress(null);
    try {
      await store.installUpdate();
    } catch (failure) {
      setError(errorMessage(failure));
      setInstalling(false);
      store.setUpdateProgress(null);
    }
  }

  // While the update downloads and installs the dialog stays open: the app restarts at the end.
  const close = () => {
    if (!installing()) props.onClose();
  };

  const footer = (
    <>
      <Show when={installing()}>
        <div class="update-progress" role="status">
          <div
            class="progress"
            role="progressbar"
            aria-label={tr("downloadLabel")}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent() ?? undefined}
          >
            <i style={{ width: `${percent() ?? 0}%` }} />
          </div>
          <span>{progressText()}</span>
        </div>
      </Show>
      <Show when={error()}>
        <p class="error-text" role="alert">
          {error()}
        </p>
      </Show>
      <div class="form__actions">
        <button type="button" class="btn" disabled={installing()} onClick={close}>
          {tr("later")}
        </button>
        <button
          type="button"
          class="btn btn--primary"
          data-autofocus
          disabled={installing()}
          onClick={() => void install()}
        >
          <Show
            when={props.update.manual}
            fallback={
              <>
                <Download class="icon" aria-hidden="true" />
                {installing() ? tr("installing") : tr("installAndRestart")}
              </>
            }
          >
            <ExternalLink class="icon" aria-hidden="true" />
            {tr("openReleasePage")}
          </Show>
        </button>
      </div>
    </>
  );

  return (
    <Modal title={tr("updateTitle")} onClose={close} footer={footer}>
      <div class="form">
        <p>
          {tr("updateMessage", {
            version: props.update.version,
            current: store.appInfo()?.version ?? null,
          })}
        </p>
        <Show when={props.update.notes}>{(notes) => <Markdown source={notes()} />}</Show>
        <Show when={props.update.manual}>
          <p>{tr("manualUpdateHint")}</p>
        </Show>
      </div>
    </Modal>
  );
}
