import { translator } from "../i18n";
import { gameChrome } from "../i18n/gameChrome";
import TriangleAlert from "lucide-solid/icons/triangle-alert";
import { store } from "../lib/store";

const tr = translator(gameChrome);

/** The bar above the library while no IGDB key is set (until dismissed). */
export function IgdbHint() {
  return (
    <div class="igdb-hint" role="status">
      <TriangleAlert class="icon" aria-hidden="true" />
      <span class="igdb-hint__text">{tr("igdbLimited")}</span>
      <button
        type="button"
        class="btn btn--small"
        onClick={() => store.showSettingsSection("metadata")}
      >
        {tr("setUp")}
      </button>
      <button
        type="button"
        class="btn btn--small btn--ghost"
        onClick={() => void store.dismissIgdbHint()}
      >
        {tr("dontShowAgain")}
      </button>
    </div>
  );
}
