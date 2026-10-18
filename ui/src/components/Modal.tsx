import { createSignal, createUniqueId, onCleanup, onMount, type ParentProps } from "solid-js";

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

interface ModalProps {
  title: string;
  onClose: () => void;
}

/** Accessible modal dialog: focus is trapped, Esc and backdrop click close it. */
export function Modal(props: ParentProps<ModalProps>) {
  const [dialog, setDialog] = createSignal<HTMLDivElement>();
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

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      props.onClose();
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
        if (event.target === event.currentTarget) props.onClose();
      }}
    >
      <div
        ref={setDialog}
        class="modal"
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
          <button class="icon-btn" type="button" aria-label="Close dialog" onClick={props.onClose}>
            ×
          </button>
        </header>
        <div class="modal__body">{props.children}</div>
      </div>
    </div>
  );
}
