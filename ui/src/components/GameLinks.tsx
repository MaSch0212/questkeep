import { translator } from "../i18n";
import { gameSections } from "../i18n/gameSections";
import BookOpen from "lucide-solid/icons/book-open";
import Globe from "lucide-solid/icons/globe";
import LifeBuoy from "lucide-solid/icons/life-buoy";
import MessagesSquare from "lucide-solid/icons/messages-square";
import Newspaper from "lucide-solid/icons/newspaper";
import ShoppingBag from "lucide-solid/icons/shopping-bag";
import Trophy from "lucide-solid/icons/trophy";
import Wrench from "lucide-solid/icons/wrench";
import { createEffect, createSignal, For, on, onCleanup, onMount, Show, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import type { GameLink } from "../bindings/GameLink";
import { linkKind, linksThatFit, type LinkKind } from "../lib/gamePage";
import { openLink } from "./settings/openLink";
import { Menu } from "./Menu";
import type { PopoverAnchor } from "./Popover";

const ICONS: Record<LinkKind, (props: { class?: string }) => JSX.Element> = {
  store: ShoppingBag,
  wiki: BookOpen,
  news: Newspaper,
  community: MessagesSquare,
  achievements: Trophy,
  workshop: Wrench,
  support: LifeBuoy,
  web: Globe,
};

function LinkIcon(props: { link: GameLink }) {
  return (
    <Dynamic
      component={ICONS[linkKind(props.link.name, props.link.url)]}
      class="icon"
      aria-hidden="true"
    />
  );
}

const tr = translator(gameSections);

/** Links that must fit with their names before the row shows names. */
const MIN_NAMED = 3;

/**
 * The game's links on one row, with their names when at least a few fit, else as icons (the
 * name is the tooltip and label). Links that don't fit collapse into "+N more" (a menu).
 */
export function GameLinks(props: { links: GameLink[] }) {
  const [visible, setVisible] = createSignal(Number.POSITIVE_INFINITY);
  const [labels, setLabels] = createSignal(false);
  const [menu, setMenu] = createSignal<PopoverAnchor | null>(null);
  let row: HTMLDivElement | undefined;
  let measure: HTMLDivElement | undefined;
  let more: HTMLButtonElement | undefined;

  // A hidden copy of the row with every link gives their widths at the current layout.
  function fit() {
    const copy = measure;
    if (!row || !copy) return;
    const widths = (selector: string) =>
      [...copy.querySelectorAll<HTMLElement>(selector)].map((i) => i.offsetWidth);
    const moreItem = copy.querySelector<HTMLElement>("[data-more]");
    const gap = Number.parseFloat(getComputedStyle(row).columnGap) || 0;
    const moreWidth = moreItem?.offsetWidth ?? 0;
    const named = linksThatFit(widths("[data-named]"), row.clientWidth, moreWidth, gap);
    // Names unless too few would fit; icons alone tell links of one kind apart poorly.
    const withNames = named >= Math.min(MIN_NAMED, props.links.length);
    setLabels(withNames);
    setVisible(
      withNames ? named : linksThatFit(widths("[data-icon]"), row.clientWidth, moreWidth, gap),
    );
  }

  onMount(() => {
    fit();
    // The row's width, and the copy's: other links, names or language change what fits while
    // the row keeps its width.
    const observer = new ResizeObserver(() => fit());
    if (row) observer.observe(row);
    if (measure) observer.observe(measure);
    onCleanup(() => observer.disconnect());
  });
  createEffect(on(() => props.links, fit, { defer: true }));

  const shown = () => props.links.slice(0, visible());
  const hidden = () => props.links.slice(visible());

  return (
    <Show when={props.links.length > 0}>
      <div class="game-links-wrap">
        <div
          ref={(el) => (row = el)}
          class="game-links"
          classList={{ "game-links--labels": labels() }}
          role="list"
          aria-label={tr("links")}
        >
          <For each={shown()}>
            {(link) => (
              <a
                role="listitem"
                class="game-link"
                href={link.url}
                title={link.name}
                aria-label={link.name}
                onClick={(event) => {
                  event.preventDefault();
                  void openLink(link.url);
                }}
              >
                <LinkIcon link={link} />
                <span class="game-link__name">{link.name}</span>
              </a>
            )}
          </For>
          <Show when={hidden().length > 0}>
            <button
              ref={(el) => (more = el)}
              type="button"
              class="game-link game-link--more"
              aria-haspopup="menu"
              aria-expanded={menu() !== null}
              onClick={() => {
                const rect = more?.getBoundingClientRect();
                setMenu(menu() || !rect ? null : { rect, align: "end" });
              }}
            >
              {tr("moreLinks", { count: hidden().length })}
            </button>
          </Show>
        </div>
        <div ref={(el) => (measure = el)} class="game-links game-links--measure" aria-hidden="true">
          <For each={props.links}>
            {(link) => (
              <span class="game-link" data-named>
                <LinkIcon link={link} />
                <span class="game-link__name">{link.name}</span>
              </span>
            )}
          </For>
          <For each={props.links}>
            {(link) => (
              <span class="game-link" data-icon>
                <LinkIcon link={link} />
              </span>
            )}
          </For>
          <span class="game-link game-link--more" data-more>
            {tr("moreLinks", { count: props.links.length })}
          </span>
        </div>
      </div>
      <Show when={menu()}>
        {(at) => (
          <Menu
            anchor={at()}
            label={tr("moreLinksLabel")}
            inside={() => [more]}
            onClose={(returnFocus) => {
              setMenu(null);
              if (returnFocus) more?.focus();
            }}
            entries={hidden().map((link) => ({
              label: link.name,
              icon: () => <LinkIcon link={link} />,
              onSelect: () => void openLink(link.url),
            }))}
          />
        )}
      </Show>
    </Show>
  );
}
