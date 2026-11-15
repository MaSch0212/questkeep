import type { JSX } from "solid-js";
import { translator } from "../i18n";
import { shellCommon } from "../i18n/shellCommon";
import { Modal } from "./Modal";

const tr = translator(shellCommon);

interface ConfirmDialogProps {
  title: string;
  message: JSX.Element;
  confirmLabel: string;
  onConfirm: () => void;
  onClose: () => void;
}

/**
 * Asks before a destructive action; Cancel has the focus, so Enter does not confirm by accident.
 * Confirming runs `onConfirm` before `onClose`: closing may unmount the owner (a context menu),
 * after which its props can no longer be read.
 */
export function ConfirmDialog(props: ConfirmDialogProps) {
  return (
    <Modal title={props.title} onClose={props.onClose}>
      <p class="confirm__message">{props.message}</p>
      <div class="form__actions">
        <button type="button" class="btn" data-autofocus onClick={props.onClose}>
          {tr("cancel")}
        </button>
        <button
          type="button"
          class="btn btn--danger-solid"
          onClick={() => {
            props.onConfirm();
            props.onClose();
          }}
        >
          {props.confirmLabel}
        </button>
      </div>
    </Modal>
  );
}
