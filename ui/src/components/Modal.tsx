import X from "lucide-solid/icons/x";
import { Portal } from "solid-js/web";
import {
  createSignal,
  createUniqueId,
  type JSX,
  onCleanup,
  onMount,
  type ParentProps,
  Show,
} from "solid-js";
import { translator } from "../i18n";
import { shellCommon } from "../i18n/shellCommon";
import { shellNotifications } from "../i18n/shellNotifications";

const tr = translator(shellCommon);
const trAsk = translator(shellNotifications);

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

interface ModalProps {
  title: string;
  /** For lists with long rows (e.g. game names side by side). */
  wide?: boolean;
  /** For a short question or a single value. */
  narrow?: boolean;
  /** A fixed height, for content that grows and shrinks while the dialog is open. */
  tall?: boolean;
  /** Stays below the scrolling body (e.g. the dialog buttons). */
  footer?: JSX.Element;
  onClose: () => void;
  /**
   * Whether there are unsaved changes. While true, Esc, a click outside and the close button
   * ask first (Save / Discard / Keep editing) instead of closing. Needs `onSave`.
   */
  dirty?: () => boolean;
  /** Saves the changes for the prompt's Save button; it closes the dialog when it succeeds. */
  onSave?: () => void;
}

/**
 * Accessible modal dialog: focus is trapped, Esc and backdrop click close it (after asking,
 * when `dirty`). A button of the dialog itself that discards (Cancel) calls `onClose` directly.
 */
export function Modal(props: ParentProps<ModalProps>) {
  const [dialog, setDialog] = createSignal<HTMLDivElement>();
  const [asking, setAsking] = createSignal(false);
  const titleId = createUniqueId();
  const previouslyFocused = document.activeElement;

  const focusables = () => Array.from(dialog()?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? []);

  onMount(() => {
    const preferred = dialog()?.querySelector<HTMLElement>("[data-autofocus]");
    (preferred ?? focusables()[0] ?? dialog())?.focus();
  });
  onCleanup(() => {
    if (previouslyFocused instanceof HTMLElement) previouslyFocused.focus();
  });

  /** Closes the dialog, or asks first when it has unsaved changes. */
  function requestClose() {
    if (props.dirty?.() === true) setAsking(true);
    else props.onClose();
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      requestClose();
      return;
    }
    if (event.key !== "Tab") return;
    const items = focusables();
    const first = items[0];
    const last = items.at(-1);
    if (!first || !last) {
      event.preventDefault();
      return;
    }
    const active = document.activeElement;
    if (event.shiftKey && (active === first || active === dialog())) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  return (
    <div
      class="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) requestClose();
      }}
    >
      <div
        ref={setDialog}
        class="modal"
        classList={{
          "modal--wide": props.wide === true,
          "modal--narrow": props.narrow === true,
          "modal--tall": props.tall === true,
        }}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabindex="-1"
        onKeyDown={onKeyDown}
      >
        <header class="modal__header">
          <h2 id={titleId} class="modal__title">
            {props.title}
          </h2>
          <button
            class="icon-btn"
            type="button"
            aria-label={tr("closeDialog")}
            onClick={requestClose}
          >
            <X class="icon" aria-hidden="true" />
          </button>
        </header>
        <div class="modal__body">{props.children}</div>
        <Show when={props.footer}>
          <footer class="modal__footer">{props.footer}</footer>
        </Show>
      </div>
      <Show when={asking()}>
        <Portal>
          {/* Outside the dialog, so its focus trap doesn't fight the prompt's. */}
          <Modal title={trAsk("unsavedTitle")} narrow onClose={() => setAsking(false)}>
            <p class="confirm__message">{trAsk("unsavedMessage")}</p>
            <div class="form__actions">
              <button type="button" class="btn" onClick={() => setAsking(false)}>
                {trAsk("keepEditing")}
              </button>
              <button
                type="button"
                class="btn btn--danger"
                onClick={() => {
                  setAsking(false);
                  props.onClose();
                }}
              >
                {trAsk("discard")}
              </button>
              <button
                type="button"
                class="btn btn--primary"
                data-autofocus
                onClick={() => {
                  setAsking(false);
                  props.onSave?.();
                }}
              >
                {trAsk("save")}
              </button>
            </div>
          </Modal>
        </Portal>
      </Show>
    </div>
  );
}
