import { createSignal } from "solid-js";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { translator } from "../../i18n";
import { settingsPage } from "../../i18n/settingsPage";
import { store } from "../../lib/store";

const tr = translator(settingsPage);

export function DeveloperSection() {
  const [seeding, setSeeding] = createSignal(false);

  async function seed() {
    setSeeding(true);
    try {
      const count = await api.seedFakeLibrary(5000);
      store.pushToast("info", tr("seeded", { count }));
      store.scheduleRefresh();
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      setSeeding(false);
    }
  }

  return (
    <div>
      <button type="button" class="btn" disabled={seeding()} onClick={() => void seed()}>
        {seeding() ? tr("seeding") : tr("seed")}
      </button>
    </div>
  );
}
