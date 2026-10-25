import type { JSX } from "solid-js";
import { Modal } from "./Modal";

interface ConfirmDialogProps {
  title: string;
  message: JSX.Element;
  confirmLabel: string;
  onConfirm: () => void;
  onClose: () => void;
}

/** Asks before a destructive action; Cancel has the focus, so Enter does not confirm by accident. */
export function ConfirmDialog(props: ConfirmDialogProps) {
  return (
    <Modal title={props.title} onClose={props.onClose}>
      <p>{props.message}</p>
      <div class="form__actions">
        <button type="button" class="btn" data-autofocus onClick={props.onClose}>
          Cancel
        </button>
        <button
          type="button"
          class="btn btn--danger-solid"
          onClick={() => {
            props.onClose();
            props.onConfirm();
          }}
        >
          {props.confirmLabel}
        </button>
      </div>
    </Modal>
  );
}
