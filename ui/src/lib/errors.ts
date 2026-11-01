import { translator } from "../i18n";
import { shellCommon } from "../i18n/shellCommon";

const tr = translator(shellCommon);

/** Turns whatever a rejected command produced into a user-facing message. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return tr("wentWrong");
}
