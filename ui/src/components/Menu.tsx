import Check from "lucide-solid/icons/check";
import ChevronRight from "lucide-solid/icons/chevron-right";
import { createSignal, Index, Match, onMount, Show, Switch, type JSX } from "solid-js";
import { nextMenuIndex } from "../lib/addGameOptions";
import { Popover, type PopoverAnchor } from "./Popover";

export type MenuEntry =
  | {
      kind?: "item";
      label: string;
      icon?: () => JSX.Element;
      /** Small text at the end of the row (a store name, a shortcut). */
      hint?: string;
      /** Shows a check mark column; `true` draws the mark. */
      checked?: boolean;
      danger?: boolean;
      disabled?: boolean;
      /** Keeps the menu open after choosing (toggles like the sort direction). */
      keepOpen?: boolean;
      onSelect: () => void;
    }
  | { kind: "submenu"; label: string; icon?: () => JSX.Element; entries: () => MenuEntry[] }
  | { kind: "separator" }
  | { kind: "heading"; label: string };

type Actionable = Extract<MenuEntry, { label: string; kind?: "item" | "submenu" }>;
type Item = Extract<MenuEntry, { onSelect: () => void }>;
type Submenu = Extract<MenuEntry, { kind: "submenu" }>;
type Heading = Extract<MenuEntry, { kind: "heading" }>;

const asItem = (e: MenuEntry): Item | undefined =>
  e.kind === undefined || e.kind === "item" ? e : undefined;
const asSubmenu = (e: MenuEntry): Submenu | undefined => (e.kind === "submenu" ? e : undefined);
const asHeading = (e: MenuEntry): Heading | undefined => (e.kind === "heading" ? e : undefined);

function isActionable(entry: MenuEntry): entry is Actionable {
  return entry.kind === undefined || entry.kind === "item" || entry.kind === "submenu";
}

export interface MenuProps {
  anchor: PopoverAnchor;
  label: string;
  entries: MenuEntry[];
  /** Closes the whole menu (and any parent menu). */
  onClose: (returnFocus: boolean) => void;
  inside?: () => (Element | undefined)[];
  /** Set for submenus: ArrowLeft goes back to the parent. */
  onBack?: () => void;
}

/**
 * A keyboard-friendly menu: arrows move, Enter/Space choose, ArrowRight opens a submenu,
 * ArrowLeft and Escape go back. Submenus also open on hover.
 */
export function Menu(props: MenuProps) {
  const buttons: (HTMLButtonElement | undefined)[] = [];
  const [openSub, setOpenSub] = createSignal<number | null>(null);
  let panel: HTMLDivElement | undefined;

  const actionable = () =>
    props.entries
      .map((entry, index) => ({ entry, index }))
      .filter(({ entry }) => isActionable(entry) && !(entry.kind !== "submenu" && entry.disabled));

  function focusAt(position: number) {
    const target = actionable()[position];
    if (target) buttons[target.index]?.focus();
  }

  onMount(() => queueMicrotask(() => focusAt(0)));

  function choose(entry: MenuEntry) {
    if (entry.kind === "separator" || entry.kind === "heading" || entry.kind === "submenu") return;
    if (!entry.keepOpen) props.onClose(true);
    entry.onSelect();
  }

  function onKeyDown(event: KeyboardEvent) {
    const list = actionable();
    const current = list.findIndex(({ index }) => buttons[index] === document.activeElement);
    const next = nextMenuIndex(event.key, current, list.length);
    if (next !== null) {
      event.preventDefault();
      focusAt(next);
      return;
    }
    const focused = list[current];
    if (event.key === "ArrowRight" && focused?.entry.kind === "submenu") {
      event.preventDefault();
      setOpenSub(focused.index);
    } else if ((event.key === "ArrowLeft" || event.key === "Escape") && props.onBack) {
      event.preventDefault();
      event.stopPropagation();
      props.onBack();
    } else if (event.key === "Tab") {
      props.onClose(false);
    }
  }

  return (
    <Popover
      anchor={props.anchor}
      class="menu"
      role="menu"
      label={props.label}
      ref={(el) => (panel = el)}
      inside={props.inside}
      onClose={props.onClose}
      onKeyDown={onKeyDown}
    >
      <Index each={props.entries}>
        {(entry, index) => (
          <Switch>
            <Match when={entry().kind === "separator"}>
              <div class="menu__separator" role="separator" />
            </Match>
            <Match when={asHeading(entry())}>
              {(heading) => <div class="menu__heading">{heading().label}</div>}
            </Match>
            <Match when={asSubmenu(entry())}>
              {(sub) => (
                <>
                  <button
                    ref={(el) => (buttons[index] = el)}
                    type="button"
                    role="menuitem"
                    class="menu__item"
                    classList={{ "menu__item--open": openSub() === index }}
                    tabIndex={-1}
                    aria-haspopup="menu"
                    aria-expanded={openSub() === index}
                    onClick={() => setOpenSub(index)}
                    onPointerEnter={() => setOpenSub(index)}
                  >
                    <span class="menu__icon">{sub().icon?.()}</span>
                    <span class="menu__label">{sub().label}</span>
                    <ChevronRight class="icon menu__arrow" aria-hidden="true" />
                  </button>
                  <Show when={openSub() === index && buttons[index]}>
                    {(button) => (
                      <Menu
                        anchor={{ rect: button().getBoundingClientRect(), align: "beside" }}
                        label={sub().label}
                        entries={sub().entries()}
                        inside={() => [panel, ...(props.inside?.() ?? [])]}
                        onClose={props.onClose}
                        onBack={() => {
                          setOpenSub(null);
                          buttons[index]?.focus();
                        }}
                      />
                    )}
                  </Show>
                </>
              )}
            </Match>
            <Match when={asItem(entry())}>
              {(item) => {
                const it = item;
                return (
                  <button
                    ref={(el) => (buttons[index] = el)}
                    type="button"
                    role={it().checked === undefined ? "menuitem" : "menuitemcheckbox"}
                    aria-checked={it().checked}
                    class="menu__item"
                    classList={{ "menu__item--danger": it().danger === true }}
                    tabIndex={-1}
                    disabled={it().disabled}
                    onPointerEnter={() => setOpenSub(null)}
                    onClick={() => choose(it())}
                  >
                    <span class="menu__icon">
                      <Show when={it().checked !== undefined} fallback={it().icon?.()}>
                        <Show when={it().checked}>
                          <Check class="icon menu__check" aria-hidden="true" />
                        </Show>
                      </Show>
                    </span>
                    <span class="menu__label">{it().label}</span>
                    <Show when={it().hint}>
                      <span class="menu__hint">{it().hint}</span>
                    </Show>
                  </button>
                );
              }}
            </Match>
          </Switch>
        )}
      </Index>
    </Popover>
  );
}
