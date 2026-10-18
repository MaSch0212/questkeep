import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage } from "../../lib/errors";
import { store } from "../../lib/store";

/** Opens a link in the user's normal browser; failures surface as a toast. */
export async function openLink(url: string) {
  try {
    await openUrl(url);
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}
