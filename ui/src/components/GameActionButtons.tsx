import Ellipsis from "lucide-solid/icons/ellipsis";
import { createSignal, For, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { translator } from "../i18n";
import { scriptsText } from "../i18n/scripts";
import { createGameActions, runAction } from "../lib/gameActions";
import { splitActions } from "../lib/scripts";
import { Menu } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { ScriptIcon } from "./ScriptIcon";

const tr = translator(scriptsText);

/**
 * The action scripts of a game on its page: the first two as buttons (icon and name), the rest
 * under "More actions". Nothing is drawn for a game without actions.
 */
export function GameActionButtons(props: { game: Game }) {
  const actions = createGameActions(() => props.game.id);
  const parts = () => splitActions(actions());
  const [menu, setMenu] = createSignal<PopoverAnchor | null>(null);
  let moreButton: HTMLButtonElement | undefined;

  return (
    <Show when={actions().length > 0}>
      <div class="script-actions" role="group" aria-label={tr("actions")}>
        <For each={parts().shown}>
          {(script) => (
            <button type="button" class="btn" onClick={() => void runAction(script, props.game.id)}>
              <ScriptIcon name={script.icon} />
              {script.name}
            </button>
          )}
        </For>
        <Show when={parts().more.length > 0}>
          <button
            ref={(el) => (moreButton = el)}
            type="button"
            class="btn"
            aria-haspopup="menu"
            aria-expanded={menu() !== null}
            onClick={() =>
              setMenu(
                menu() === null && moreButton
                  ? { rect: moreButton.getBoundingClientRect(), align: "start" }
                  : null,
              )
            }
          >
            <Ellipsis class="icon" aria-hidden="true" />
            {tr("moreActions")}
          </button>
        </Show>
      </div>
      <Show when={menu()}>
        {(anchor) => (
          <Menu
            anchor={anchor()}
            label={tr("moreActions")}
            inside={() => [moreButton]}
            onClose={(returnFocus) => {
              setMenu(null);
              if (returnFocus) moreButton?.focus();
            }}
            entries={parts().more.map((script) => ({
              label: script.name,
              icon: () => <ScriptIcon name={script.icon} />,
              onSelect: () => void runAction(script, props.game.id),
            }))}
          />
        )}
      </Show>
    </Show>
  );
}
