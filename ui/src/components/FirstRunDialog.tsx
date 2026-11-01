import { createEffect, createSignal, For, Match, on, onMount, Switch } from "solid-js";
import { translator } from "../i18n";
import { setup } from "../i18n/setup";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { AccountsSection } from "./settings/AccountsSection";
import { GridDbField, IgdbFields, IgdbSteps } from "./settings/KeyFields";
import { createSettingsModel } from "./settings/useSettings";
import { Modal } from "./Modal";

const tr = translator(setup);

type Step = "welcome" | "stores" | "keys" | "warn";

/** What stays empty without IGDB; shown before the setup ends without a key. */
const WITHOUT_IGDB = ["without1", "without2", "without3", "without4"] as const;

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
    <Modal title={tr("title")} wide onClose={onClose}>
      <div class="setup" onKeyDown={onKeyDown}>
        <div class="setup__dots" aria-label={tr("step", { n: dots() })} role="img">
          <For each={[1, 2, 3]}>
            {(n) => <span class="setup__dot" classList={{ "setup__dot--on": n <= dots() }} />}
          </For>
        </div>
        <div class="setup__content">
          <Switch>
            <Match when={step() === "welcome"}>
              <h3 class="setup__title">{tr("welcomeTitle")}</h3>
              <p class="muted">{tr("welcomeText")}</p>
            </Match>
            <Match when={step() === "stores"}>
              <h3 class="setup__title">{tr("storesTitle")}</h3>
              <AccountsSection model={model} />
            </Match>
            <Match when={step() === "keys"}>
              <h3 class="setup__title">{tr("keysTitle")}</h3>
              <p class="muted">{tr("keysIntro")}</p>
              <section class="setup-key" aria-labelledby="setup-igdb">
                <header class="setup-key__head">
                  <h4 id="setup-igdb">{tr("igdbTitle")}</h4>
                  <span class="setup-key__badge setup-key__badge--accent">{tr("recommended")}</span>
                </header>
                <p class="muted">{tr("igdbDesc")}</p>
                <IgdbSteps />
                <IgdbFields model={model} />
              </section>
              <section class="setup-key" aria-labelledby="setup-griddb">
                <header class="setup-key__head">
                  <h4 id="setup-griddb">SteamGridDB</h4>
                  <span class="setup-key__badge">{tr("optional")}</span>
                </header>
                <p class="muted">{tr("griddbDesc")}</p>
                <GridDbField model={model} />
              </section>
            </Match>
            <Match when={step() === "warn"}>
              <h3 class="setup__title">{tr("warnTitle")}</h3>
              <div class="setup__warn">
                <strong>{tr("warnStrong")}</strong>
                <ul>
                  <For each={WITHOUT_IGDB}>{(item) => <li>{tr(item)}</li>}</For>
                </ul>
              </div>
              <p class="muted">{tr("warnHint")}</p>
            </Match>
          </Switch>
        </div>
        <footer class="setup__foot">
          <Switch>
            <Match when={step() === "welcome"}>
              <button type="button" class="btn btn--ghost" onClick={skip}>
                {tr("skipSetup")}
              </button>
              <button
                ref={(el) => (primary = el)}
                type="button"
                class="btn btn--primary"
                data-autofocus
                onClick={() => setStep("stores")}
              >
                {tr("getStarted")}
              </button>
            </Match>
            <Match when={step() === "stores"}>
              <button type="button" class="btn btn--ghost" onClick={() => setStep("welcome")}>
                {tr("back")}
              </button>
              <button
                ref={(el) => (primary = el)}
                type="button"
                class="btn btn--primary"
                onClick={() => setStep("keys")}
              >
                {tr("next")}
              </button>
            </Match>
            <Match when={step() === "keys"}>
              <button type="button" class="btn btn--ghost" onClick={() => setStep("stores")}>
                {tr("back")}
              </button>
              <span class="setup__buttons">
                <button type="button" class="btn btn--ghost" onClick={skip}>
                  {tr("skip")}
                </button>
                <button
                  ref={(el) => (primary = el)}
                  type="button"
                  class="btn btn--primary"
                  disabled={saving()}
                  onClick={onFinish}
                >
                  {saving() ? tr("saving") : tr("finish")}
                </button>
              </span>
            </Match>
            <Match when={step() === "warn"}>
              <button type="button" class="btn btn--ghost" onClick={() => setStep("keys")}>
                {tr("backToKeys")}
              </button>
              <button
                ref={(el) => (primary = el)}
                type="button"
                class="btn btn--primary"
                disabled={saving()}
                onClick={() => void finish(saveOnContinue())}
              >
                {tr("continueWithout")}
              </button>
            </Match>
          </Switch>
        </footer>
      </div>
    </Modal>
  );
}
