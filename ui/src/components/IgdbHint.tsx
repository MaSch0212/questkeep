import TriangleAlert from "lucide-solid/icons/triangle-alert";
import { store } from "../lib/store";

/** The bar above the library while no IGDB key is set (until dismissed). */
export function IgdbHint() {
  return (
    <div class="igdb-hint" role="status">
      <TriangleAlert class="icon" aria-hidden="true" />
      <span class="igdb-hint__text">Game details are limited: no IGDB key is set.</span>
      <button
        type="button"
        class="btn btn--small"
        onClick={() => store.showSettingsSection("metadata")}
      >
        Set up
      </button>
      <button
        type="button"
        class="btn btn--small btn--ghost"
        onClick={() => void store.dismissIgdbHint()}
      >
        Don&apos;t show again
      </button>
    </div>
  );
}
