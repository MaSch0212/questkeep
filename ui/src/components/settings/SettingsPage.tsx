import { For, onMount, type Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import { translator } from "../../i18n";
import { settingsPage } from "../../i18n/settingsPage";
import { store } from "../../lib/store";
import { AboutSection } from "./AboutSection";
import { AccountsSection } from "./AccountsSection";
import { ArtworkSection } from "./ArtworkSection";
import { BackupsSection } from "./BackupsSection";
import { CategoriesSection } from "./CategoriesSection";
import { CompletionSection } from "./CompletionSection";
import { DeveloperSection } from "./DeveloperSection";
import { GeneralSection } from "./GeneralSection";
import { MetadataSection } from "./MetadataSection";
import { UpdatesSection } from "./UpdatesSection";
import { createSettingsModel, type SettingsModel } from "./useSettings";

const tr = translator(settingsPage);

interface SectionDef {
  id: string;
  title:
    | "general"
    | "stores"
    | "categories"
    | "completion"
    | "artwork"
    | "metadata"
    | "developer"
    | "backups"
    | "updates"
    | "about";
  /** Hidden sections are left out of the page and the section nav. */
  visible?: () => boolean;
  component: Component<{ model: SettingsModel }>;
}

/** To add a settings section: write one component and add one entry here. */
const SECTIONS: SectionDef[] = [
  { id: "general", title: "general", component: GeneralSection },
  { id: "stores", title: "stores", component: AccountsSection },
  { id: "categories", title: "categories", component: CategoriesSection },
  { id: "completion", title: "completion", component: CompletionSection },
  { id: "artwork", title: "artwork", component: ArtworkSection },
  { id: "metadata", title: "metadata", component: MetadataSection },
  {
    id: "developer",
    title: "developer",
    visible: () => store.appInfo()?.devTools === true,
    component: DeveloperSection,
  },
  { id: "backups", title: "backups", component: BackupsSection },
  { id: "updates", title: "updates", component: UpdatesSection },
  { id: "about", title: "about", component: AboutSection },
];

const sectionDomId = (id: string) => `settings-${id}`;
const visibleSections = () => SECTIONS.filter((s) => s.visible?.() ?? true);

function jumpTo(id: string) {
  document.getElementById(sectionDomId(id))?.scrollIntoView?.({ behavior: "smooth" });
}

export function SettingsPage() {
  const model = createSettingsModel();

  onMount(() => {
    const section = store.takeSettingsSection();
    if (section) document.getElementById(sectionDomId(section))?.scrollIntoView?.();
  });

  return (
    <section class="settings" aria-label={tr("settings")}>
      <header class="topbar">
        <h1 class="topbar__title">{tr("settings")}</h1>
      </header>
      <div class="settings__body">
        <nav class="settings__nav" aria-label={tr("sections")}>
          <ul>
            <For each={visibleSections()}>
              {(section) => (
                <li>
                  <button type="button" class="nav-item" onClick={() => jumpTo(section.id)}>
                    {tr(section.title)}
                  </button>
                </li>
              )}
            </For>
          </ul>
        </nav>
        <div class="settings__scroll">
          <div class="settings__sections">
            <For each={visibleSections()}>
              {(section) => (
                <section
                  id={sectionDomId(section.id)}
                  class="settings-section"
                  aria-labelledby={`${sectionDomId(section.id)}-heading`}
                >
                  <h2 id={`${sectionDomId(section.id)}-heading`} class="settings-section__title">
                    {tr(section.title)}
                  </h2>
                  <Dynamic component={section.component} model={model} />
                </section>
              )}
            </For>
          </div>
        </div>
      </div>
    </section>
  );
}
