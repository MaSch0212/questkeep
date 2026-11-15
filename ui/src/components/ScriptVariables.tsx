import ChevronsUpDown from "lucide-solid/icons/chevrons-up-down";
import { createSignal, For, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import type { ScriptVariable } from "../bindings/ScriptVariable";
import { translator } from "../i18n";
import { scriptsText } from "../i18n/scripts";
import { groupVariables, type VariableSyntax } from "../lib/scripts";
import { GamePicker } from "./GamePicker";
import type { PopoverAnchor } from "./Popover";

const tr = translator(scriptsText);

/**
 * The variables a script gets, with their values for a chosen game. A click inserts one at the
 * cursor of the field or editor the user was in (`onInsert`); the buttons keep the focus there.
 */
export function ScriptVariables(props: {
  variables: readonly ScriptVariable[];
  game: Game | undefined;
  onGame: (game: Game) => void;
  syntax: VariableSyntax;
  /** The env / placeholder switch only matters for the code editor. */
  canSwitch: boolean;
  onSyntax: (syntax: VariableSyntax) => void;
  onInsert: (variable: ScriptVariable) => void;
}) {
  const [filter, setFilter] = createSignal("");
  const [picking, setPicking] = createSignal<PopoverAnchor | null>(null);
  const groups = () => groupVariables(props.variables, filter());
  const name = (variable: ScriptVariable) =>
    props.syntax === "env" && props.canSwitch ? variable.env : `{${variable.placeholder}}`;
  const placeholders = () => !props.canSwitch || props.syntax === "placeholder";

  return (
    <aside class="script-vars" aria-label={tr("variables")}>
      <div class="script-vars__head">
        <h3 class="script-vars__title">{tr("variables")}</h3>
        <Show when={props.canSwitch}>
          <div class="segmented segmented--sm" role="radiogroup" aria-label={tr("syntaxLabel")}>
            <button
              type="button"
              role="radio"
              class="segmented__option"
              aria-checked={props.syntax === "env"}
              data-tooltip={tr("syntaxEnvTip")}
              onClick={() => props.onSyntax("env")}
            >
              {tr("syntaxEnv")}
            </button>
            <button
              type="button"
              role="radio"
              class="segmented__option"
              aria-checked={props.syntax === "placeholder"}
              data-tooltip={tr("syntaxPlaceholderTip")}
              onClick={() => props.onSyntax("placeholder")}
            >
              {tr("syntaxPlaceholder")}
            </button>
          </div>
        </Show>
      </div>
      <input
        type="search"
        class="input"
        placeholder={tr("filterVars")}
        aria-label={tr("filterVars")}
        value={filter()}
        onInput={(event) => setFilter(event.currentTarget.value)}
      />
      <div class="script-vars__game">
        <span class="field__label">{tr("valuesFor")}</span>
        <button
          type="button"
          class="btn script-vars__pick"
          aria-haspopup="dialog"
          onClick={(event) =>
            setPicking({ rect: event.currentTarget.getBoundingClientRect(), align: "below" })
          }
        >
          <span class="script-vars__pick-name">{props.game?.name ?? tr("pickGame")}</span>
          <ChevronsUpDown class="icon" aria-hidden="true" />
        </button>
      </div>
      <div class="script-vars__list">
        <Show when={props.game !== undefined} fallback={<p class="muted">{tr("noValuesGame")}</p>}>
          <For each={groups()} fallback={<p class="muted">{tr("noVars")}</p>}>
            {(group) => (
              <section>
                <h4 class="script-vars__group">{tr(`group_${group.group}` as const)}</h4>
                <For each={group.items}>
                  {(variable) => (
                    <button
                      type="button"
                      class="script-var"
                      aria-label={tr("insertVar", { name: name(variable) })}
                      // Keeps the focus (and the cursor) in the field the text goes into.
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => props.onInsert(variable)}
                    >
                      <code class="script-var__name">{name(variable)}</code>
                      <small class="script-var__value">{variable.value}</small>
                    </button>
                  )}
                </For>
              </section>
            )}
          </For>
        </Show>
      </div>
      <p class="muted script-vars__hint">
        {placeholders() ? tr("varsHintPlaceholder") : tr("varsHintEnv")}
      </p>
      <Show when={picking()}>
        {(anchor) => (
          <GamePicker anchor={anchor()} onPick={props.onGame} onClose={() => setPicking(null)} />
        )}
      </Show>
    </aside>
  );
}
