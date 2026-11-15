import Bell from "lucide-solid/icons/bell";
import X from "lucide-solid/icons/x";
import { createEffect, createSignal, For, on, Show } from "solid-js";
import type { Notification } from "../bindings/Notification";
import type { NotificationAction } from "../bindings/NotificationAction";
import { translator } from "../i18n";
import { shellNotifications } from "../i18n/shellNotifications";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import {
  actionLabel,
  badgeKind,
  badgeText,
  notificationTime,
  sourceLabel,
  unreadCount,
} from "../lib/notifications";
import { store } from "../lib/store";
import { Popover, type PopoverAnchor } from "./Popover";

const tr = translator(shellNotifications);

const count = () => unreadCount(store.notifications());
const kind = () => badgeKind(store.notifications());

/** What an entry's button does. */
function runAction(action: NotificationAction) {
  switch (action.type) {
    case "openFile":
      void Promise.resolve()
        .then(() => api.openPath(action.path))
        .catch((error: unknown) => store.pushToast("error", errorMessage(error)));
      break;
    case "storeSettings":
      store.showSettingsSection("stores");
      break;
    case "editScript":
      store.openScriptEditor(action.id);
      break;
  }
}

/**
 * The bell in the title bar: a badge with the unread count (red when one is an error) and a
 * popover with the entries, newest first. Opening it marks everything read; the entries that
 * were new stay highlighted until it closes. Ctrl+Shift+N, the command bar and a clicked system
 * notification open it too (`store.showNotifications`).
 */
export function NotificationBell() {
  const [anchor, setAnchor] = createSignal<PopoverAnchor | null>(null);
  /** The entries that were unread when the popover opened. */
  const [fresh, setFresh] = createSignal<ReadonlySet<string>>(new Set());
  let button: HTMLButtonElement | undefined;

  function open() {
    const rect = button?.getBoundingClientRect();
    if (!rect) return;
    setFresh(
      new Set(
        store
          .notifications()
          .filter((n) => !n.read)
          .map((n) => n.id),
      ),
    );
    setAnchor({ rect, align: "end" });
    store.markNotificationsRead();
  }

  function close(returnFocus: boolean) {
    setAnchor(null);
    if (returnFocus) button?.focus();
  }

  createEffect(
    on(
      store.bellRequests,
      () => {
        if (anchor() === null) open();
      },
      { defer: true },
    ),
  );

  // Entries that arrive while the popover is open are read at once, as everything shown is.
  createEffect(() => {
    if (anchor() !== null && count() > 0) store.markNotificationsRead();
  });

  const label = () => (count() > 0 ? tr("bellUnread", { count: count() }) : tr("bell"));

  return (
    <>
      <span class="titlebar__bell">
        <button
          ref={(el) => (button = el)}
          type="button"
          class="titlebar__tool"
          classList={{ "titlebar__tool--open": anchor() !== null }}
          aria-label={label()}
          data-tooltip={tr("bellHint")}
          aria-haspopup="dialog"
          aria-expanded={anchor() !== null}
          onClick={() => (anchor() === null ? open() : close(false))}
        >
          <Bell class="icon" aria-hidden="true" />
        </button>
        <Show when={kind()}>
          {(badge) => (
            <span
              class="titlebar__badge"
              classList={{ "titlebar__badge--error": badge() === "error" }}
              aria-hidden="true"
            >
              {badgeText(count())}
            </span>
          )}
        </Show>
      </span>
      <Show when={anchor()}>
        {(at) => (
          <Popover
            anchor={at()}
            class="notifications"
            role="dialog"
            label={tr("bell")}
            inside={() => [button]}
            onClose={close}
          >
            <div class="notifications__head">
              <h3 class="notifications__title">{tr("bell")}</h3>
              <button
                type="button"
                class="btn btn--ghost btn--small"
                disabled={store.notifications().length === 0}
                onClick={() => store.clearNotifications()}
              >
                {tr("clearAll")}
              </button>
            </div>
            <Show
              when={store.notifications().length > 0}
              fallback={<p class="notifications__empty">{tr("empty")}</p>}
            >
              <ul class="notifications__list">
                <For each={store.notifications()}>
                  {(entry) => <Entry entry={entry} unread={fresh().has(entry.id)} onRun={close} />}
                </For>
              </ul>
            </Show>
          </Popover>
        )}
      </Show>
    </>
  );
}

function Entry(props: {
  entry: Notification;
  unread: boolean;
  onRun: (returnFocus: boolean) => void;
}) {
  return (
    <li class="notif" classList={{ "notif--unread": props.unread }}>
      <span
        class="notif__dot"
        classList={{ "notif__dot--error": props.entry.level === "error" }}
        role="img"
        aria-label={props.entry.level === "error" ? tr("errorLabel") : tr("warningLabel")}
      />
      <div class="notif__main">
        <span>{props.entry.message}</span>
        <small class="notif__meta">
          {notificationTime(props.entry.time)} · {sourceLabel(props.entry.source)}
        </small>
        <Show when={props.entry.action}>
          {(action) => (
            <button
              type="button"
              class="btn btn--small"
              onClick={() => {
                props.onRun(false);
                runAction(action());
              }}
            >
              {actionLabel(action())}
            </button>
          )}
        </Show>
      </div>
      <button
        type="button"
        class="icon-btn icon-btn--small"
        aria-label={tr("dismissOne", { message: props.entry.message })}
        data-tooltip={tr("dismissHint")}
        onClick={() => store.dismissNotification(props.entry.id)}
      >
        <X class="icon" aria-hidden="true" />
      </button>
    </li>
  );
}
