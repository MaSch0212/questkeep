import BookOpen from "lucide-solid/icons/book-open";
import Globe from "lucide-solid/icons/globe";
import LifeBuoy from "lucide-solid/icons/life-buoy";
import MessagesSquare from "lucide-solid/icons/messages-square";
import Newspaper from "lucide-solid/icons/newspaper";
import ShoppingBag from "lucide-solid/icons/shopping-bag";
import Trophy from "lucide-solid/icons/trophy";
import Wrench from "lucide-solid/icons/wrench";
import { createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
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

/**
 * The game's links on one row. Links that don't fit collapse into "+N more" (a menu). In the
 * narrow layout the chips show only their icon (CSS), with the name as tooltip and label.
 */
export function GameLinks(props: { links: GameLink[] }) {
  const [visible, setVisible] = createSignal(Number.POSITIVE_INFINITY);
  const [menu, setMenu] = createSignal<PopoverAnchor | null>(null);
  let row: HTMLDivElement | undefined;
  let measure: HTMLDivElement | undefined;
  let more: HTMLButtonElement | undefined;

  // A hidden copy of the row with every link gives their widths at the current layout.
  function fit() {
    if (!row || !measure) return;
    const items = [...measure.querySelectorAll<HTMLElement>("[data-link]")];
    const moreItem = measure.querySelector<HTMLElement>("[data-more]");
    const gap = Number.parseFloat(getComputedStyle(row).columnGap) || 0;
    setVisible(
      linksThatFit(
        items.map((i) => i.offsetWidth),
        row.clientWidth,
        moreItem?.offsetWidth ?? 0,
        gap,
      ),
    );
  }

  onMount(() => {
    fit();
    const observer = new ResizeObserver(() => fit());
    if (row) observer.observe(row);
    onCleanup(() => observer.disconnect());
  });

  const shown = () => props.links.slice(0, visible());
  const hidden = () => props.links.slice(visible());

  return (
    <Show when={props.links.length > 0}>
      <div class="game-links-wrap">
        <div ref={(el) => (row = el)} class="game-links" role="list" aria-label="Links">
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
              +{hidden().length} more
            </button>
          </Show>
        </div>
        <div ref={(el) => (measure = el)} class="game-links game-links--measure" aria-hidden="true">
          <For each={props.links}>
            {(link) => (
              <span class="game-link" data-link>
                <LinkIcon link={link} />
                <span class="game-link__name">{link.name}</span>
              </span>
            )}
          </For>
          <span class="game-link game-link--more" data-more>
            +{props.links.length} more
          </span>
        </div>
      </div>
      <Show when={menu()}>
        {(at) => (
          <Menu
            anchor={at()}
            label="More links"
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
