import { For, type Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import { store } from "../../lib/store";
import { AboutSection } from "./AboutSection";
import { AccountsSection } from "./AccountsSection";
import { ArtworkSection } from "./ArtworkSection";
import { DeveloperSection } from "./DeveloperSection";
import { GeneralSection } from "./GeneralSection";
import { LibrarySection } from "./LibrarySection";
import { MetadataSection } from "./MetadataSection";
import { SteamSection } from "./SteamSection";
import { UpdatesSection } from "./UpdatesSection";
import { createSettingsModel, type SettingsModel } from "./useSettings";

interface SectionDef {
  id: string;
  title: string;
  /** Hidden sections are left out of the page and the section nav. */
  visible?: () => boolean;
  component: Component<{ model: SettingsModel }>;
}

/** To add a settings section: write one component and add one entry here. */
const SECTIONS: SectionDef[] = [
  { id: "general", title: "General", component: GeneralSection },
  { id: "library", title: "Library and stores", component: LibrarySection },
  { id: "steam", title: "Steam API key", component: SteamSection },
  { id: "accounts", title: "Accounts", component: AccountsSection },
  { id: "artwork", title: "Artwork", component: ArtworkSection },
  { id: "metadata", title: "Metadata", component: MetadataSection },
  {
    id: "developer",
    title: "Developer",
    visible: () => store.appInfo()?.devTools === true,
    component: DeveloperSection,
  },
  { id: "updates", title: "Updates", component: UpdatesSection },
  { id: "about", title: "About", component: AboutSection },
];

const sectionDomId = (id: string) => `settings-${id}`;
const visibleSections = () => SECTIONS.filter((s) => s.visible?.() ?? true);

function jumpTo(id: string) {
  document.getElementById(sectionDomId(id))?.scrollIntoView?.({ behavior: "smooth" });
}

export function SettingsPage() {
  const model = createSettingsModel();

  return (
    <section class="settings" aria-label="Settings">
      <header class="topbar">
        <h1 class="topbar__title">Settings</h1>
      </header>
      <div class="settings__body">
        <nav class="settings__nav" aria-label="Settings sections">
          <ul>
            <For each={visibleSections()}>
              {(section) => (
                <li>
                  <button type="button" class="nav-item" onClick={() => jumpTo(section.id)}>
                    {section.title}
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
                    {section.title}
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
