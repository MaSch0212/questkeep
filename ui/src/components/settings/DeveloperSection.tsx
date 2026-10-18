import { createSignal } from "solid-js";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { store } from "../../lib/store";

export function DeveloperSection() {
  const [seeding, setSeeding] = createSignal(false);

  async function seed() {
    setSeeding(true);
    try {
      const count = await api.seedFakeLibrary(5000);
      store.pushToast("info", `Seeded ${count} fake games`);
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
        {seeding() ? "Seeding…" : "Seed 5,000 fake games"}
      </button>
    </div>
  );
}
