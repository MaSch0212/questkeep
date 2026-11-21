import { createEffect, createMemo, createSignal, For, Show, type Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import { translator } from "../../i18n";
import { settingsPage } from "../../i18n/settingsPage";
import { store } from "../../lib/store";
import { AboutSection } from "./AboutSection";
import { AccountsSection } from "./AccountsSection";
import { AppearanceSection } from "./AppearanceSection";
import { ArtworkSection } from "./ArtworkSection";
import { BackupsSection } from "./BackupsSection";
import { CategoriesSection } from "./CategoriesSection";
import { CompletionSection } from "./CompletionSection";
import { DeveloperSection } from "./DeveloperSection";
import { GeneralSection } from "./GeneralSection";
import { GroupMergesSection } from "./GroupMergesSection";
import { MetadataSection } from "./MetadataSection";
import { PluginSettings, pluginsWithSettings } from "./PluginSettings";
import { PluginsSection } from "./PluginsSection";
import { PluginUpdateCheck } from "./PluginUpdateCheck";
import { PriceSitesSection } from "./PriceSitesSection";
import { ScriptsSection } from "./ScriptsSection";
import { SourcePriority } from "./SourcePriority";
import { UpdatesSection } from "./UpdatesSection";
import { createSettingsModel, type SettingsModel } from "./useSettings";
import { WindowTraySection } from "./WindowTraySection";

const tr = translator(settingsPage);

type TitleKey =
  | "general"
  | "appearance"
  | "windowTray"
  | "stores"
  | "organizing"
  | "categories"
  | "groupMerges"
  | "completion"
  | "artworkMetadata"
  | "artwork"
  | "metadata"
  | "sourcePriority"
  | "priceSites"
  | "scripts"
  | "plugins"
  | "developer"
  | "backups"
  | "updatesAbout"
  | "updates"
  | "about";

/** One block of a page; its id is also what other parts of the app ask for (`showSettingsSection`). */
interface PartDef {
  id: string;
  title: TitleKey;
  component: Component<{ model: SettingsModel }>;
}

interface PageDef {
  id: string;
  /** A message key, or the text itself (a plugin name). */
  title: TitleKey | (() => string);
  group: "app" | "library" | "system";
  /** Hidden pages are left out of the nav. */
  visible?: () => boolean;
  parts: PartDef[];
}

/** To add a settings page: write one component and add one entry here. */
const PAGES: PageDef[] = [
  {
    id: "general",
    title: "general",
    group: "app",
    parts: [{ id: "general", title: "general", component: GeneralSection }],
  },
  {
    id: "appearance",
    title: "appearance",
    group: "app",
    parts: [{ id: "appearance", title: "appearance", component: AppearanceSection }],
  },
  {
    id: "windowTray",
    title: "windowTray",
    group: "app",
    parts: [{ id: "windowTray", title: "windowTray", component: WindowTraySection }],
  },
  {
    id: "stores",
    title: "stores",
    group: "library",
    parts: [{ id: "stores", title: "stores", component: AccountsSection }],
  },
  {
    id: "organizing",
    title: "organizing",
    group: "library",
    parts: [
      { id: "categories", title: "categories", component: CategoriesSection },
      { id: "groupMerges", title: "groupMerges", component: GroupMergesSection },
      { id: "completion", title: "completion", component: CompletionSection },
    ],
  },
  {
    id: "artworkMetadata",
    title: "artworkMetadata",
    group: "library",
    parts: [
      { id: "artwork", title: "artwork", component: ArtworkSection },
      { id: "metadata", title: "metadata", component: MetadataSection },
      { id: "sourcePriority", title: "sourcePriority", component: SourcePriority },
    ],
  },
  {
    id: "priceSites",
    title: "priceSites",
    group: "library",
    parts: [{ id: "priceSites", title: "priceSites", component: PriceSitesSection }],
  },
  {
    id: "scripts",
    title: "scripts",
    group: "library",
    parts: [{ id: "scripts", title: "scripts", component: ScriptsSection }],
  },
  {
    id: "plugins",
    title: "plugins",
    group: "system",
    parts: [
      {
        id: "plugins",
        title: "plugins",
        component: (p) => <PluginsSection updateCheck={<PluginUpdateCheck model={p.model} />} />,
      },
    ],
  },
  {
    id: "developer",
    title: "developer",
    group: "system",
    visible: () => store.appInfo()?.devTools === true,
    parts: [{ id: "developer", title: "developer", component: DeveloperSection }],
  },
  {
    id: "backups",
    title: "backups",
    group: "system",
    parts: [{ id: "backups", title: "backups", component: BackupsSection }],
  },
  {
    id: "updatesAbout",
    title: "updatesAbout",
    group: "system",
    parts: [
      { id: "updates", title: "updates", component: UpdatesSection },
      { id: "about", title: "about", component: AboutSection },
    ],
  },
];

const GROUPS = ["app", "library", "system"] as const;
const GROUP_TITLES = {
  app: "groupApp",
  library: "groupLibrary",
  system: "groupSystem",
} as const;

/** One page per plugin with settings; kept, so the page does not rebuild them. */
const pluginPages = new Map<string, PageDef>();
function pluginPage(pluginId: string): PageDef {
  let page = pluginPages.get(pluginId);
  if (!page) {
    const id = `plugin-${pluginId}`;
    page = {
      id,
      title: () => store.plugins().find((p) => p.id === pluginId)?.name ?? pluginId,
      group: "system",
      parts: [{ id, title: "plugins", component: () => <PluginSettings plugin={pluginId} /> }],
    };
    pluginPages.set(pluginId, page);
  }
  return page;
}

const partDomId = (id: string) => `settings-${id}`;
const pageTitle = (page: PageDef) =>
  typeof page.title === "function" ? page.title() : tr(page.title);
const visiblePages = () => [
  ...PAGES.filter((p) => p.visible?.() ?? true),
  ...pluginsWithSettings().map(pluginPage),
];

/** The settings: one page at a time, the pages grouped in the nav. */
export function SettingsPage() {
  const model = createSettingsModel();
  const [currentId, setCurrentId] = createSignal<string>(PAGES[0]?.id ?? "general");
  const current = createMemo(
    () => visiblePages().find((p) => p.id === currentId()) ?? visiblePages()[0],
  );

  /** Shows the page that holds the page or part `id` and scrolls to the part. */
  function show(id: string) {
    const page = visiblePages().find((p) => p.id === id || p.parts.some((part) => part.id === id));
    if (!page) return;
    setCurrentId(page.id);
    queueMicrotask(() => document.getElementById(partDomId(id))?.scrollIntoView?.());
  }

  // A section asked for from elsewhere (a link in the app, or one inside the settings).
  createEffect(() => {
    const section = store.takeSettingsSection();
    if (section) show(section);
  });

  return (
    <section class="settings" aria-label={tr("settings")}>
      <header class="topbar">
        <h1 class="topbar__title">{tr("settings")}</h1>
      </header>
      <div class="settings__body">
        <nav class="settings__nav" aria-label={tr("sections")}>
          <For each={GROUPS}>
            {(group) => (
              <Show when={visiblePages().some((p) => p.group === group)}>
                <h2 class="settings__group">{tr(GROUP_TITLES[group])}</h2>
                <ul>
                  <For each={visiblePages().filter((p) => p.group === group)}>
                    {(page) => (
                      <li>
                        <button
                          type="button"
                          class="nav-item"
                          classList={{ "nav-item--active": current()?.id === page.id }}
                          aria-current={current()?.id === page.id ? "page" : undefined}
                          onClick={() => setCurrentId(page.id)}
                        >
                          {pageTitle(page)}
                        </button>
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
            )}
          </For>
        </nav>
        <div class="settings__scroll">
          <Show when={current()} keyed>
            {(page) => (
              <div class="settings__sections">
                <h2 class="visually-hidden">{pageTitle(page)}</h2>
                <For each={page.parts}>
                  {(part) => (
                    <section
                      id={partDomId(part.id)}
                      class="settings-section"
                      aria-labelledby={
                        page.parts.length > 1 ? `${partDomId(part.id)}-heading` : undefined
                      }
                      aria-label={page.parts.length > 1 ? undefined : pageTitle(page)}
                    >
                      <Show when={page.parts.length > 1}>
                        <h3 id={`${partDomId(part.id)}-heading`} class="settings-part__title">
                          {tr(part.title)}
                        </h3>
                      </Show>
                      <div classList={{ "settings-card": page.parts.length > 1 }}>
                        <Dynamic component={part.component} model={model} />
                      </div>
                    </section>
                  )}
                </For>
              </div>
            )}
          </Show>
        </div>
      </div>
    </section>
  );
}
