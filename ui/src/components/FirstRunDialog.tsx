import { createEffect, createSignal, For, Match, on, onMount, Switch } from "solid-js";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { AccountsSection } from "./settings/AccountsSection";
import { GridDbField, IgdbFields, IgdbSteps } from "./settings/KeyFields";
import { createSettingsModel } from "./settings/useSettings";
import { Modal } from "./Modal";

type Step = "welcome" | "stores" | "keys" | "warn";

/** What stays empty without IGDB; shown before the setup ends without a key. */
const WITHOUT_IGDB = [
  "Time to beat (only HowLongToBeat's numbers remain, when it finds the game)",
  "Website, wiki and social links (store links still work)",
  "Series, age ratings, game modes, themes, perspectives, and filters on them",
  "Details for games from stores without metadata (Ubisoft, EA, Battle.net, added by hand)",
];

/**
 * The first-run setup: welcome, stores, metadata keys. Shown once on a new database; every step
 * can be skipped. Ending it without an IGDB key first says what stays empty.
 */
export function FirstRunDialog() {
  const model = createSettingsModel();
  const [step, setStep] = createSignal<Step>("welcome");
  /** Whether "Continue without" saves the other keys (came from Finish, not from Skip). */
  const [saveOnContinue, setSaveOnContinue] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  let primary: HTMLButtonElement | undefined;

  // Closing the app during the setup counts as skipping it: it never shows again.
  onMount(() => void api.finishSetup().catch(() => undefined));

  const hasIgdb = () =>
    model.igdbClientId().trim() !== "" && model.igdbClientSecret().trim() !== "";

  function skip() {
    setSaveOnContinue(false);
    setStep("warn");
  }

  async function finish(save: boolean) {
    if (saving()) return;
    setSaving(true);
    const saved = !save || (await model.saveKeys());
    setSaving(false);
    if (saved) await store.endSetup();
  }

  function onFinish() {
    if (hasIgdb()) {
      void finish(true);
    } else {
      setSaveOnContinue(true);
      setStep("warn");
    }
  }

  /** Escape (and the close button): skip, or step back from the warning. */
  function onClose() {
    if (step() === "warn") setStep("keys");
    else skip();
  }

  /** Enter outside buttons and links presses the primary button. */
  function onKeyDown(event: KeyboardEvent) {
    if (event.key !== "Enter" || event.defaultPrevented) return;
    const target = event.target as HTMLElement;
    if (target.closest("button, a, textarea, select")) return;
    // Inputs of the store list (codes, credentials) keep their own Enter.
    if (step() === "stores" && target instanceof HTMLInputElement) return;
    event.preventDefault();
    primary?.click();
  }

  // Each step starts on its primary button, so Enter and Tab work right away.
  createEffect(on(step, () => queueMicrotask(() => primary?.focus()), { defer: true }));

  const dots = () => (step() === "welcome" ? 1 : step() === "stores" ? 2 : 3);

  return (
    <Modal title="Set up Questkeep" wide onClose={onClose}>
      <div class="setup" onKeyDown={onKeyDown}>
        <div class="setup__dots" aria-label={`Step ${dots()} of 3`} role="img">
          <For each={[1, 2, 3]}>
            {(n) => <span class="setup__dot" classList={{ "setup__dot--on": n <= dots() }} />}
          </For>
        </div>
        <div class="setup__content">
          <Switch>
            <Match when={step() === "welcome"}>
              <h3 class="setup__title">Welcome to Questkeep</h3>
              <p class="muted">
                All your PC games from every store in one library. Questkeep already looks for
                installed stores; the next two steps let you sign in to stores and set up optional
                keys for richer game details. Everything here can be changed later in Settings.
              </p>
            </Match>
            <Match when={step() === "stores"}>
              <h3 class="setup__title">Your stores</h3>
              <AccountsSection model={model} />
            </Match>
            <Match when={step() === "keys"}>
              <h3 class="setup__title">Richer game details (optional)</h3>
              <p class="muted">
                Questkeep gets most details from your stores. Two free services add more; both need
                a key from your own free account, stored only in your system&apos;s credential
                store.
              </p>
              <section class="setup-key" aria-labelledby="setup-igdb">
                <header class="setup-key__head">
                  <h4 id="setup-igdb">IGDB (via Twitch)</h4>
                  <span class="setup-key__badge setup-key__badge--accent">Recommended</span>
                </header>
                <p class="muted">
                  Time to beat, wiki and social links, series, age ratings, game modes, themes, and
                  details for games your stores know little about.
                </p>
                <IgdbSteps />
                <IgdbFields model={model} />
              </section>
              <section class="setup-key" aria-labelledby="setup-griddb">
                <header class="setup-key__head">
                  <h4 id="setup-griddb">SteamGridDB</h4>
                  <span class="setup-key__badge">Optional</span>
                </header>
                <p class="muted">
                  Covers for games without one, and more covers, backgrounds and icons in the image
                  picker.
                </p>
                <GridDbField model={model} />
              </section>
            </Match>
            <Match when={step() === "warn"}>
              <h3 class="setup__title">Continue without IGDB?</h3>
              <div class="setup__warn">
                <strong>Without an IGDB key these stay empty:</strong>
                <ul>
                  <For each={WITHOUT_IGDB}>{(item) => <li>{item}</li>}</For>
                </ul>
              </div>
              <p class="muted">
                You can add the key any time in Settings → Metadata, then use &quot;Refresh metadata
                for all games&quot;.
              </p>
            </Match>
          </Switch>
        </div>
        <footer class="setup__foot">
          <Switch>
            <Match when={step() === "welcome"}>
              <button type="button" class="btn btn--ghost" onClick={skip}>
                Skip setup
              </button>
              <button
                ref={(el) => (primary = el)}
                type="button"
                class="btn btn--primary"
                data-autofocus
                onClick={() => setStep("stores")}
              >
                Get started
              </button>
            </Match>
            <Match when={step() === "stores"}>
              <button type="button" class="btn btn--ghost" onClick={() => setStep("welcome")}>
                Back
              </button>
              <button
                ref={(el) => (primary = el)}
                type="button"
                class="btn btn--primary"
                onClick={() => setStep("keys")}
              >
                Next
              </button>
            </Match>
            <Match when={step() === "keys"}>
              <button type="button" class="btn btn--ghost" onClick={() => setStep("stores")}>
                Back
              </button>
              <span class="setup__buttons">
                <button type="button" class="btn btn--ghost" onClick={skip}>
                  Skip
                </button>
                <button
                  ref={(el) => (primary = el)}
                  type="button"
                  class="btn btn--primary"
                  disabled={saving()}
                  onClick={onFinish}
                >
                  {saving() ? "Saving…" : "Finish"}
                </button>
              </span>
            </Match>
            <Match when={step() === "warn"}>
              <button type="button" class="btn btn--ghost" onClick={() => setStep("keys")}>
                Back to keys
              </button>
              <button
                ref={(el) => (primary = el)}
                type="button"
                class="btn btn--primary"
                disabled={saving()}
                onClick={() => void finish(saveOnContinue())}
              >
                Continue without
              </button>
            </Match>
          </Switch>
        </footer>
      </div>
    </Modal>
  );
}
