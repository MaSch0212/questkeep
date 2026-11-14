import ChevronDown from "lucide-solid/icons/chevron-down";
import ChevronUp from "lucide-solid/icons/chevron-up";
import Globe from "lucide-solid/icons/globe";
import Plus from "lucide-solid/icons/plus";
import Trash from "lucide-solid/icons/trash";
import { createResource, createSignal, For, Show } from "solid-js";
import type { PriceSite } from "../../bindings/PriceSite";
import { intlLocale, translator } from "../../i18n";
import { shopping } from "../../i18n/shopping";
import * as api from "../../lib/api";
import { checkSite, moveSite, openSuggestions, type SiteProblem } from "../../lib/priceSites";
import type { SettingsModel } from "./useSettings";

const tr = translator(shopping);

const problemText = (problem: SiteProblem) =>
  problem === "duplicate" ? tr("siteDuplicate") : tr("siteInvalid");

/** The country's name in the UI language (`DE` → "Germany"). */
function regionName(code: string): string {
  try {
    return new Intl.DisplayNames([intlLocale()], { type: "region" }).of(code) ?? code;
  } catch {
    return code;
  }
}

/**
 * Price comparison websites: the list (reorder, remove), a form to add one, and suggestions for
 * the user's region. They are offered in the + menu and open in the user's browser.
 * Every change is saved at once.
 */
export function PriceSitesSection(props: { model: SettingsModel }) {
  const [name, setName] = createSignal("");
  const [address, setAddress] = createSignal("");
  const [problem, setProblem] = createSignal<SiteProblem | null>(null);
  const [suggestions] = createResource(() => api.priceSiteSuggestions());
  const sites = () => props.model.settings()?.priceSites ?? [];
  const save = (priceSites: PriceSite[]) => void props.model.update({ priceSites });

  function add(event: SubmitEvent) {
    event.preventDefault();
    const checked = checkSite(sites(), name(), address());
    if ("problem" in checked) {
      setProblem(checked.problem);
      return;
    }
    save([...sites(), checked.site]);
    setName("");
    setAddress("");
    setProblem(null);
  }

  const open = () => {
    const loaded = suggestions();
    return loaded ? openSuggestions(loaded, sites()) : [];
  };

  return (
    <div class="price-sites">
      <p class="muted">{tr("sitesIntro")}</p>
      <Show when={sites().length > 0} fallback={<p class="muted">{tr("noSites")}</p>}>
        <ul class="price-sites__list" aria-label={tr("sitesList")}>
          <For each={sites()}>
            {(site, index) => (
              <li class="price-sites__item">
                <Globe class="icon" aria-hidden="true" />
                <span class="price-sites__text">
                  <span class="price-sites__name">{site.name}</span>
                  <small class="muted price-sites__url">{site.url}</small>
                </span>
                <button
                  type="button"
                  class="icon-btn"
                  aria-label={tr("moveUp", { name: site.name })}
                  data-tooltip={tr("moveUp", { name: site.name })}
                  disabled={index() === 0}
                  onClick={() => save(moveSite(sites(), index(), -1))}
                >
                  <ChevronUp class="icon" aria-hidden="true" />
                </button>
                <button
                  type="button"
                  class="icon-btn"
                  aria-label={tr("moveDown", { name: site.name })}
                  data-tooltip={tr("moveDown", { name: site.name })}
                  disabled={index() === sites().length - 1}
                  onClick={() => save(moveSite(sites(), index(), 1))}
                >
                  <ChevronDown class="icon" aria-hidden="true" />
                </button>
                <button
                  type="button"
                  class="icon-btn"
                  aria-label={tr("removeSite", { name: site.name })}
                  data-tooltip={tr("removeSite", { name: site.name })}
                  onClick={() => save(sites().filter((_, i) => i !== index()))}
                >
                  <Trash class="icon" aria-hidden="true" />
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <form class="price-sites__add" onSubmit={add}>
        <input
          class="input price-sites__name-input"
          aria-label={tr("siteName")}
          placeholder={tr("siteName")}
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
        />
        <input
          class="input price-sites__address"
          aria-label={tr("siteAddress")}
          placeholder="https://isthereanydeal.com/"
          aria-invalid={problem() !== null}
          value={address()}
          onInput={(e) => {
            setAddress(e.currentTarget.value);
            setProblem(null);
          }}
        />
        <button type="submit" class="btn">
          {tr("addSite")}
        </button>
      </form>
      <Show when={problem()}>
        {(p) => (
          <p class="error-text" role="alert">
            {problemText(p())}
          </p>
        )}
      </Show>
      <Show when={open().length > 0}>
        <p class="muted price-sites__suggested">
          {suggestions()?.region
            ? tr("suggestedFor", { region: regionName(suggestions()?.region ?? "") })
            : tr("suggested")}
        </p>
        <ul class="price-sites__suggestions" aria-label={tr("suggested")}>
          <For each={open()}>
            {(site) => (
              <li>
                <button
                  type="button"
                  class="btn btn--small btn--with-icon"
                  data-tooltip={site.url}
                  onClick={() => save([...sites(), site])}
                >
                  <Plus class="icon" aria-hidden="true" />
                  {site.name}
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
}
