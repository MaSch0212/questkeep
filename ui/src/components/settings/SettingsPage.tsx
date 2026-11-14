import { createSignal, For, onCleanup, onMount, type Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import { translator } from "../../i18n";
import { settingsPage } from "../../i18n/settingsPage";
import { activeSection } from "../../lib/scrollSpy";
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
import { PluginSettings, pluginsWithSettings } from "./PluginSettings";
import { PluginsSection } from "./PluginsSection";
import { PriceSitesSection } from "./PriceSitesSection";
import { UpdatesSection } from "./UpdatesSection";
import { createSettingsModel, type SettingsModel } from "./useSettings";

const tr = translator(settingsPage);

interface SectionDef {
  id: string;
  /** A message key, or the text itself (a plugin's name). */
  title:
    | (() => string)
    | "general"
    | "stores"
    | "categories"
    | "completion"
    | "artwork"
    | "metadata"
    | "priceSites"
    | "plugins"
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
  { id: "priceSites", title: "priceSites", component: PriceSitesSection },
  { id: "plugins", title: "plugins", component: PluginsSection },
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

/** One section per plugin with settings; kept, so the page does not rebuild them. */
const pluginSections = new Map<string, SectionDef>();
function pluginSection(pluginId: string): SectionDef {
  let section = pluginSections.get(pluginId);
  if (!section) {
    section = {
      id: `plugin-${pluginId}`,
      title: () => store.plugins().find((p) => p.id === pluginId)?.name ?? pluginId,
      component: () => <PluginSettings plugin={pluginId} />,
    };
    pluginSections.set(pluginId, section);
  }
  return section;
}

const sectionDomId = (id: string) => `settings-${id}`;
const sectionTitle = (section: SectionDef) =>
  typeof section.title === "function" ? section.title() : tr(section.title);
const visibleSections = () => [
  ...SECTIONS.filter((s) => s.visible?.() ?? true),
  ...pluginsWithSettings().map(pluginSection),
];

export function SettingsPage() {
  const model = createSettingsModel();
  let scroller: HTMLDivElement | undefined;
  /** The section in the middle of the view, marked in the menu. */
  const [current, setCurrent] = createSignal<string | null>(visibleSections()[0]?.id ?? null);
  /** A menu jump scrolls smoothly past other sections: the menu keeps its pick until it stops. */
  let jumping: ReturnType<typeof setTimeout> | undefined;

  function follow() {
    if (!scroller || jumping !== undefined) return;
    const view = scroller.getBoundingClientRect();
    const boxes = visibleSections().flatMap((section) => {
      const element = document.getElementById(sectionDomId(section.id));
      return element
        ? [{ id: section.id, top: element.getBoundingClientRect().top - view.top }]
        : [];
    });
    const atEnd = scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 2;
    const id = activeSection(boxes, scroller.clientHeight, atEnd);
    if (id !== null) setCurrent(id);
  }

  function endJump() {
    clearTimeout(jumping);
    jumping = undefined;
  }

  function jumpTo(id: string) {
    setCurrent(id);
    clearTimeout(jumping);
    // `scrollend` ends it; the timer covers a jump that doesn't scroll at all.
    jumping = setTimeout(endJump, 1000);
    document.getElementById(sectionDomId(id))?.scrollIntoView?.({ behavior: "smooth" });
  }

  onMount(() => {
    const section = store.takeSettingsSection();
    if (section) {
      document.getElementById(sectionDomId(section))?.scrollIntoView?.();
      setCurrent(section);
    }
  });
  onCleanup(() => clearTimeout(jumping));

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
                  <button
                    type="button"
                    class="nav-item"
                    classList={{ "nav-item--active": current() === section.id }}
                    aria-current={current() === section.id ? "location" : undefined}
                    onClick={() => jumpTo(section.id)}
                  >
                    {sectionTitle(section)}
                  </button>
                </li>
              )}
            </For>
          </ul>
        </nav>
        <div
          class="settings__scroll"
          ref={(el) => (scroller = el)}
          onScroll={follow}
          on:scrollend={endJump}
        >
          <div class="settings__sections">
            <For each={visibleSections()}>
              {(section) => (
                <section
                  id={sectionDomId(section.id)}
                  class="settings-section"
                  aria-labelledby={`${sectionDomId(section.id)}-heading`}
                >
                  <h2 id={`${sectionDomId(section.id)}-heading`} class="settings-section__title">
                    {sectionTitle(section)}
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
