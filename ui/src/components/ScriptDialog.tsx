import Copy from "lucide-solid/icons/copy";
import Play from "lucide-solid/icons/play";
import SquareTerminal from "lucide-solid/icons/square-terminal";
import {
  createMemo,
  createResource,
  createSignal,
  createUniqueId,
  For,
  Match,
  onCleanup,
  Show,
  Switch,
} from "solid-js";
import { Portal } from "solid-js/web";
import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import type { PreTiming } from "../bindings/PreTiming";
import type { RuntimeId } from "../bindings/RuntimeId";
import type { Script } from "../bindings/Script";
import type { ScriptInput } from "../bindings/ScriptInput";
import type { ScriptKind } from "../bindings/ScriptKind";
import type { ScriptSource } from "../bindings/ScriptSource";
import type { ScriptVariable } from "../bindings/ScriptVariable";
import { translator } from "../i18n";
import { scriptsText } from "../i18n/scripts";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { insertAtCursor } from "../lib/fieldInsert";
import { pickDirectory, pickExecutable, pickScriptFile } from "../lib/pickers";
import { defaultQuery } from "../lib/query";
import { blankScript, cleanInput, inputOf, sameInput, scriptIssue } from "../lib/scriptDraft";
import {
  commandPreview,
  effectiveRuntime,
  insertionText,
  runtimeForPath,
  sampleGame,
  type VariableSyntax,
} from "../lib/scripts";
import {
  applyRunEvent,
  onScriptRunEvent,
  type ScriptRun,
  startRun,
  succeeded,
} from "../lib/scriptRun";
import { store } from "../lib/store";
import { activeFilterCount } from "../lib/views";
import { AutoFilterEditor } from "./AutoFilterEditor";
import { CodeEditor, type CodeEditorHandle } from "./CodeEditor";
import { ConfirmDialog } from "./ConfirmDialog";
import { GamePicker } from "./GamePicker";
import { Modal } from "./Modal";
import type { PopoverAnchor } from "./Popover";
import { filterText } from "./ScriptList";
import { ScriptRunOutput } from "./ScriptRunOutput";
import { iconLabel, SCRIPT_ICON_NAMES, ScriptIcon } from "./ScriptIcon";
import { ScriptVariables } from "./ScriptVariables";

const tr = translator(scriptsText);

const KINDS: readonly (readonly [ScriptKind, "runsPre" | "runsPost" | "runsAction"])[] = [
  ["pre", "runsPre"],
  ["post", "runsPost"],
  ["action", "runsAction"],
];
const TIMINGS: readonly (readonly [PreTiming, "whenBefore" | "whenRunning"])[] = [
  ["beforeStart", "whenBefore"],
  ["onceRunning", "whenRunning"],
];
const SOURCES: readonly (readonly [ScriptSource, "srcProgram" | "srcScript" | "srcLinked"])[] = [
  ["program", "srcProgram"],
  ["inline", "srcScript"],
  ["linked", "srcLinked"],
];

/** A row of mutually exclusive choices as a segmented control. */
function Choice<T extends string>(props: {
  labelId: string;
  options: readonly (readonly [T, string])[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div class="segmented" role="radiogroup" aria-labelledby={props.labelId}>
      <For each={props.options}>
        {([value, text]) => (
          <button
            type="button"
            role="radio"
            class="segmented__option"
            aria-checked={props.value === value}
            onClick={() => props.onChange(value)}
          >
            {text}
          </button>
        )}
      </For>
    </div>
  );
}

/** The filter of a script: the auto category's filter editor with the games that match. */
function ScriptFilterDialog(props: {
  query: GameQuery;
  onApply: (query: GameQuery) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = createSignal(props.query);
  return (
    <Modal
      title={tr("filterTitle")}
      wide
      tall
      onClose={props.onClose}
      footer={
        <div class="form__actions">
          <button type="button" class="btn" onClick={() => props.onClose()}>
            {tr("cancel")}
          </button>
          <button
            type="button"
            class="btn btn--primary"
            onClick={() => {
              props.onApply(query());
              props.onClose();
            }}
          >
            {tr("filterApply")}
          </button>
        </div>
      }
    >
      <AutoFilterEditor query={query()} onChange={setQuery} intro={tr("filterIntro")} />
    </Modal>
  );
}

/**
 * The dialog that creates or edits a script (Settings › Scripts, a game's Edit dialog, the
 * notification bell). Esc and a click outside ask before discarding changes; Cancel doesn't.
 */
export function ScriptDialog(props: {
  /** The script to edit; null creates one of `kind`. */
  script: Script | null;
  kind?: ScriptKind;
  /** A game's own script: the game is fixed and "For" is hidden. */
  gameId?: string | null;
  onClose: () => void;
  onSaved?: (script: Script) => void;
}) {
  const ids = createUniqueId();
  const start: ScriptInput = props.script
    ? inputOf(props.script)
    : blankScript(props.kind ?? "pre", props.gameId ?? null);
  const [draft, setDraft] = createSignal<ScriptInput>(start);
  const [error, setError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);
  const [filtering, setFiltering] = createSignal(false);
  const [deleting, setDeleting] = createSignal(false);
  const [replacing, setReplacing] = createSignal<string | null>(null);
  const [runPicker, setRunPicker] = createSignal<PopoverAnchor | null>(null);
  /** The last run started here ("Run now"), and whether its output shows. */
  const [run, setRun] = createSignal<ScriptRun | null>(null);
  const [showRun, setShowRun] = createSignal(false);
  let runButton: HTMLButtonElement | undefined;
  let outputButton: HTMLButtonElement | undefined;
  const stopListening = onScriptRunEvent((event) => {
    const current = run();
    if (current) setRun(applyRunEvent(current, event));
  });
  onCleanup(() => {
    stopListening();
    // A run nobody can see any more doesn't keep going.
    const current = run();
    if (current && !current.ended) void api.stopScriptRun(current.id).catch(() => undefined);
  });
  const runAnchor = (): PopoverAnchor => ({
    rect: (runButton ?? document.body).getBoundingClientRect(),
    align: "start",
  });
  const [valuesGame, setValuesGame] = createSignal<Game | undefined>(
    sampleGame(store.games(), start.gameId),
  );
  const [syntax, setSyntax] = createSignal<VariableSyntax>(
    start.source === "inline" ? "env" : "placeholder",
  );
  let editor: CodeEditorHandle | undefined;
  /** The text field a variable goes into when the editor does not have the focus. */
  let lastField: HTMLInputElement | undefined;
  let argsField: HTMLInputElement | undefined;
  let editorFocused = false;

  const [runtimes] = createResource(async () => {
    try {
      return await api.scriptRuntimes(false);
    } catch {
      return [];
    }
  });
  const installed = () => runtimes() ?? [];
  const [variables] = createResource(
    () => {
      const game = valuesGame();
      return game ? ([game.id, draft().kind] as const) : null;
    },
    async ([id, kind]): Promise<ScriptVariable[]> => {
      try {
        return await api.scriptVariables(id, kind);
      } catch {
        return [];
      }
    },
  );
  const vars = () => variables() ?? [];
  /** How many games the script's filter matches (the hint under "For"). */
  const [matchCount] = createResource(
    () => {
      const query = draft().filter;
      return query !== null && activeFilterCount(query) > 0 ? query : null;
    },
    async (query) => {
      try {
        return (await api.listGames({ ...query, search: null })).length;
      } catch {
        return null;
      }
    },
  );

  const dirty = () => !saving() && !sameInput(draft(), start);
  const isNew = () => draft().id === null;
  const ownScript = () => draft().gameId !== null;
  const runtime = createMemo<RuntimeId | null>(() => effectiveRuntime(draft(), installed()));
  const preview = createMemo(() => commandPreview(draft(), vars()));
  const filterHint = () => {
    const summary = filterLine();
    if (summary === null) return tr("filterNone");
    const count = matchCount();
    return typeof count === "number" ? tr("filterMatches", { summary, count }) : summary;
  };
  const filterLine = createMemo(() => {
    const query = draft().filter;
    return query ? filterText(query) : null;
  });

  function patch(change: Partial<ScriptInput>) {
    setError(null);
    setDraft((current) => ({ ...current, ...change }));
  }
  const patchOptions = (change: Partial<ScriptInput["options"]>) =>
    patch({ options: { ...draft().options, ...change } });

  function setKind(kind: ScriptKind) {
    const wasAction = draft().kind === "action";
    patch({
      kind,
      // Actions don't wait by default; the other kinds do.
      options: {
        ...draft().options,
        wait: kind === "action" ? false : wasAction ? true : draft().options.wait,
      },
      icon: kind === "action" ? (draft().icon ?? "zap") : draft().icon,
    });
  }

  function setSource(source: ScriptSource) {
    patch({ source, content: source === "inline" ? (draft().content ?? "") : draft().content });
    setSyntax(source === "inline" ? "env" : "placeholder");
  }

  async function chooseProgram() {
    const path = await pickExecutable();
    if (path !== null) patch({ program: path });
  }

  async function chooseScriptFile() {
    const path = await pickScriptFile();
    if (path !== null) patch({ program: path });
  }

  async function chooseFolder() {
    const path = await pickDirectory();
    if (path !== null) patch({ workingDir: path });
  }

  async function importTo(path: string, text: string) {
    const known = runtimeForPath(path, installed(), false);
    patch({ source: "inline", content: text, program: "", runtime: known ?? runtime() });
    setSyntax("env");
  }

  /** Import file…: the file's text replaces the editor's (after asking, unless it is empty). */
  async function importFile() {
    const path = await pickScriptFile();
    if (path === null) return;
    try {
      const text = await api.readScriptFile(path);
      if ((draft().content ?? "").trim() === "") await importTo(path, text);
      else setReplacing(path);
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  async function confirmReplace(path: string) {
    try {
      await importTo(path, await api.readScriptFile(path));
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  /** Import a copy: reads the linked file and keeps its text as a script of the library. */
  async function importCopy() {
    const path = draft().program.trim();
    if (path === "") return;
    try {
      await importTo(path, await api.readScriptFile(path));
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  function insertVariable(variable: ScriptVariable) {
    const inEditor = draft().source === "inline" && (editorFocused || lastField === undefined);
    if (inEditor) {
      editor?.insert(insertionText(variable, syntax(), runtime(), "editor"));
      return;
    }
    const field = lastField && lastField.isConnected ? lastField : argsField;
    if (field) insertAtCursor(field, insertionText(variable, "placeholder", runtime(), "field"));
  }

  function rememberField(event: FocusEvent) {
    const target = event.target;
    if (target instanceof HTMLInputElement && target.dataset.insert !== undefined) {
      lastField = target;
      editorFocused = false;
    }
  }

  async function save() {
    const input = cleanInput(draft(), installed());
    const issue = scriptIssue(draft(), installed());
    if (issue !== null) {
      setError(issue);
      return;
    }
    setSaving(true);
    try {
      const saved = await api.saveScript(input);
      props.onSaved?.(saved);
      props.onClose();
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  async function runNow(game: { id: string; name: string }) {
    const issue = scriptIssue(draft(), installed());
    if (issue !== null) {
      setError(issue);
      return;
    }
    setError(null);
    const previous = run();
    if (previous && !previous.ended) await api.stopScriptRun(previous.id).catch(() => undefined);
    // Listening starts before the run does: its id is chosen here.
    const id = crypto.randomUUID();
    setRun(startRun(id, draft().name.trim(), { id: game.id, name: game.name }));
    setShowRun(true);
    try {
      await api.runScriptDraft(cleanInput(draft(), installed()), game.id, id);
    } catch (failure) {
      const current = run();
      if (current?.id === id) {
        setRun({ ...current, ended: true, error: errorMessage(failure) });
      }
    }
  }

  function stopRun() {
    const current = run();
    if (current && !current.ended) void api.stopScriptRun(current.id).catch(() => undefined);
  }

  async function copyCommand() {
    try {
      await navigator.clipboard.writeText(preview().command);
      store.pushToast("info", tr("copied"));
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  async function remove() {
    const id = draft().id;
    if (id === null) return;
    try {
      await api.deleteScript(id);
      props.onClose();
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  const title = () =>
    isNew() ? (draft().kind === "action" ? tr("titleNewAction") : tr("titleNew")) : tr("titleEdit");

  const footer = (
    <div class="script-dialog__footer">
      <div class="script-dialog__footer-start">
        <Show when={!isNew()}>
          <button
            type="button"
            class="btn btn--ghost btn--danger"
            onClick={() => setDeleting(true)}
          >
            {tr("delete")}
          </button>
        </Show>
        <button
          ref={(el) => (runButton = el)}
          type="button"
          class="btn"
          aria-haspopup="dialog"
          onClick={() => {
            setShowRun(false);
            setRunPicker(runAnchor());
          }}
        >
          <Play class="icon" aria-hidden="true" />
          {tr("runNow")}
        </button>
        <Show when={run()}>
          {(last) => (
            <button
              ref={(el) => (outputButton = el)}
              type="button"
              class="btn btn--ghost run-output-btn"
              classList={{
                "run-output-btn--running": !last().ended,
                "run-output-btn--failed": last().ended && !succeeded(last()),
              }}
              aria-haspopup="dialog"
              aria-expanded={showRun()}
              onClick={() => setShowRun((open) => !open)}
            >
              <SquareTerminal class="icon" aria-hidden="true" />
              {tr("runOutput")}
            </button>
          )}
        </Show>
      </div>
      <Show when={error()}>
        <p class="error-text script-dialog__error" role="alert">
          {error()}
        </p>
      </Show>
      <div class="script-dialog__footer-end">
        <button type="button" class="btn" onClick={() => props.onClose()}>
          {tr("cancel")}
        </button>
        <button
          type="button"
          class="btn btn--primary"
          disabled={saving()}
          onClick={() => void save()}
        >
          {isNew() ? tr("add") : tr("save")}
        </button>
      </div>
    </div>
  );

  return (
    <Modal
      title={title()}
      wide
      tall
      onClose={props.onClose}
      footer={footer}
      dirty={dirty}
      onSave={() => void save()}
    >
      <div class="script-dialog" onFocusIn={rememberField}>
        <div class="script-dialog__form">
          <div class="script-dialog__grid">
            <div class="field">
              <label class="field__label" for={`${ids}-name`}>
                {tr("name")}
              </label>
              <input
                id={`${ids}-name`}
                class="input"
                data-autofocus
                value={draft().name}
                onInput={(event) => patch({ name: event.currentTarget.value })}
              />
            </div>
            <div class="field">
              <span class="field__label" id={`${ids}-kind`}>
                {tr("runs")}
              </span>
              <Choice
                labelId={`${ids}-kind`}
                options={KINDS.map(([value, key]) => [value, tr(key)] as const)}
                value={draft().kind}
                onChange={setKind}
              />
            </div>
            <Show when={!ownScript()}>
              <div class="field">
                <span class="field__label" id={`${ids}-for`}>
                  {tr("for")}
                </span>
                <div class="field__row script-dialog__for">
                  <Choice
                    labelId={`${ids}-for`}
                    options={[
                      ["all", tr("forAll")],
                      ["matching", tr("forMatching")],
                    ]}
                    value={draft().filter === null ? "all" : "matching"}
                    onChange={(value) => {
                      patch({
                        filter: value === "all" ? null : (draft().filter ?? defaultQuery()),
                      });
                      if (value === "matching" && filterLine() === null) setFiltering(true);
                    }}
                  />
                  <Show when={draft().filter !== null}>
                    <button type="button" class="btn" onClick={() => setFiltering(true)}>
                      {tr("filterButton")}
                    </button>
                  </Show>
                </div>
                <Show when={draft().filter !== null}>
                  <span class="field__hint">{filterHint()}</span>
                </Show>
              </div>
            </Show>
            <Show when={draft().kind === "pre"}>
              <div class="field">
                <span class="field__label" id={`${ids}-when`}>
                  {tr("when")}
                </span>
                <Choice
                  labelId={`${ids}-when`}
                  options={TIMINGS.map(([value, key]) => [value, tr(key)] as const)}
                  value={draft().timing}
                  onChange={(timing) => patch({ timing })}
                />
                <span class="field__hint">{tr("whenHint")}</span>
              </div>
            </Show>
            <Show when={draft().kind === "action"}>
              <div class="field">
                <span class="field__label" id={`${ids}-icon`}>
                  {tr("icon")}
                </span>
                <div class="script-icons" role="radiogroup" aria-labelledby={`${ids}-icon`}>
                  <For each={SCRIPT_ICON_NAMES}>
                    {(name) => (
                      <button
                        type="button"
                        role="radio"
                        class="script-icons__icon"
                        aria-checked={draft().icon === name}
                        aria-label={iconLabel(name)}
                        data-tooltip={iconLabel(name)}
                        onClick={() => patch({ icon: name })}
                      >
                        <ScriptIcon name={name} />
                      </button>
                    )}
                  </For>
                </div>
                <span class="field__hint">{tr("iconHint")}</span>
              </div>
            </Show>
          </div>

          <div class="field">
            <span class="field__label" id={`${ids}-source`}>
              {tr("whatItRuns")}
            </span>
            <Choice
              labelId={`${ids}-source`}
              options={SOURCES.map(([value, key]) => [value, tr(key)] as const)}
              value={draft().source}
              onChange={setSource}
            />
          </div>

          <Switch>
            <Match when={draft().source === "program"}>
              <div class="field">
                <label class="field__label" for={`${ids}-program`}>
                  {tr("program")}
                </label>
                <div class="field__row">
                  <input
                    id={`${ids}-program`}
                    class="input"
                    value={draft().program}
                    onInput={(event) => patch({ program: event.currentTarget.value })}
                  />
                  <button type="button" class="btn" onClick={() => void chooseProgram()}>
                    {tr("browse")}
                  </button>
                </div>
              </div>
            </Match>
            <Match when={draft().source === "inline"}>
              <div class="script-editor">
                <div class="script-editor__bar">
                  <Show
                    when={installed().length > 0}
                    fallback={<span class="error-text">{tr("noRuntimes")}</span>}
                  >
                    <select
                      class="input script-editor__runtime"
                      aria-label={tr("runtime")}
                      value={runtime() ?? ""}
                      onChange={(event) =>
                        patch({ runtime: event.currentTarget.value as RuntimeId })
                      }
                    >
                      <For each={installed()}>
                        {(info) => <option value={info.id}>{info.name}</option>}
                      </For>
                    </select>
                  </Show>
                  <span class="field__hint script-editor__where">{tr("libraryCopy")}</span>
                  <button type="button" class="btn btn--ghost" onClick={() => void importFile()}>
                    {tr("importFile")}
                  </button>
                </div>
                <CodeEditor
                  value={draft().content ?? ""}
                  onChange={(text) => patch({ content: text })}
                  runtime={runtime()}
                  variables={vars()}
                  label={tr("editorLabel")}
                  onFocus={() => (editorFocused = true)}
                  ref={(handle) => (editor = handle)}
                />
              </div>
            </Match>
            <Match when={draft().source === "linked"}>
              <div class="field">
                <label class="field__label" for={`${ids}-file`}>
                  {tr("scriptFile")}
                </label>
                <div class="field__row">
                  <input
                    id={`${ids}-file`}
                    class="input"
                    value={draft().program}
                    onInput={(event) => patch({ program: event.currentTarget.value })}
                  />
                  <button type="button" class="btn" onClick={() => void chooseScriptFile()}>
                    {tr("browse")}
                  </button>
                  <button
                    type="button"
                    class="btn btn--ghost"
                    disabled={draft().program.trim() === ""}
                    onClick={() => void importCopy()}
                  >
                    {tr("importCopy")}
                  </button>
                </div>
              </div>
              <div class="field">
                <label class="field__label" for={`${ids}-runwith`}>
                  {tr("runWith")}
                </label>
                <div class="field__row script-dialog__runwith">
                  <Show
                    when={installed().length > 0}
                    fallback={<span class="error-text">{tr("noRuntimes")}</span>}
                  >
                    <select
                      id={`${ids}-runwith`}
                      class="input script-editor__runtime"
                      value={runtime() ?? ""}
                      onChange={(event) =>
                        patch({ runtime: event.currentTarget.value as RuntimeId })
                      }
                    >
                      <For each={installed()}>
                        {(info) => <option value={info.id}>{info.name}</option>}
                      </For>
                    </select>
                    <span class="field__hint">{tr("runWithHint")}</span>
                  </Show>
                </div>
              </div>
            </Match>
          </Switch>

          <div class="field">
            <label class="field__label" for={`${ids}-args`}>
              {draft().source === "inline" ? tr("scriptArgs") : tr("arguments")}
            </label>
            <input
              id={`${ids}-args`}
              ref={(el) => (argsField = el)}
              class="input"
              data-insert=""
              value={draft().args}
              onInput={(event) => patch({ args: event.currentTarget.value })}
            />
            <Show when={draft().source !== "inline"}>
              <span class="field__hint">{tr("argsHint")}</span>
            </Show>
          </div>

          <Show when={draft().source !== "inline"}>
            <div class="script-preview" role="group" aria-label={tr("previewNoGame")}>
              <div class="script-preview__head">
                <span class="field__label">
                  {valuesGame()
                    ? tr("preview", { game: valuesGame()?.name ?? "" })
                    : tr("previewNoGame")}
                </span>
                <button
                  type="button"
                  class="icon-btn"
                  aria-label={tr("copyCommand")}
                  data-tooltip={tr("copyCommand")}
                  onClick={() => void copyCommand()}
                >
                  <Copy class="icon" aria-hidden="true" />
                </button>
              </div>
              <code class="script-preview__command">{preview().command || " "}</code>
              <Show when={preview().workingDir !== ""}>
                <span class="field__hint">
                  {tr("workingFolderLine", { dir: preview().workingDir })}
                </span>
              </Show>
            </div>
          </Show>

          <details class="script-advanced">
            <summary>{tr("advanced")}</summary>
            <div class="script-advanced__body">
              <label class="check">
                <input
                  type="checkbox"
                  checked={draft().options.showWindow}
                  onChange={(event) => patchOptions({ showWindow: event.currentTarget.checked })}
                />
                {tr("showWindow")}
              </label>
              <label class="check">
                <input
                  type="checkbox"
                  checked={draft().options.wait}
                  onChange={(event) => patchOptions({ wait: event.currentTarget.checked })}
                />
                {tr("wait")}
              </label>
              <div class="field__row script-advanced__timeout">
                <label class="field__label" for={`${ids}-timeout`}>
                  {tr("timeout")}
                </label>
                <input
                  id={`${ids}-timeout`}
                  class="input script-advanced__number"
                  type="number"
                  min="0"
                  value={draft().options.timeoutSeconds}
                  onInput={(event) =>
                    patchOptions({ timeoutSeconds: Number(event.currentTarget.value) || 0 })
                  }
                />
                <span class="field__hint">{tr("timeoutHint")}</span>
              </div>
              <Show when={draft().kind === "pre"}>
                <label class="check">
                  <input
                    type="checkbox"
                    checked={draft().options.stopLaunchOnFailure}
                    onChange={(event) =>
                      patchOptions({ stopLaunchOnFailure: event.currentTarget.checked })
                    }
                  />
                  {tr("stopLaunch")}
                </label>
              </Show>
              <div class="field">
                <label class="field__label" for={`${ids}-dir`}>
                  {tr("workingFolder")}
                </label>
                <div class="field__row">
                  <input
                    id={`${ids}-dir`}
                    class="input"
                    data-insert=""
                    placeholder={tr("workingPlaceholder")}
                    value={draft().workingDir ?? ""}
                    onInput={(event) => patch({ workingDir: event.currentTarget.value })}
                  />
                  <button type="button" class="btn" onClick={() => void chooseFolder()}>
                    {tr("browse")}
                  </button>
                </div>
              </div>
            </div>
          </details>
        </div>

        <ScriptVariables
          variables={vars()}
          game={valuesGame()}
          onGame={setValuesGame}
          syntax={syntax()}
          canSwitch={draft().source === "inline"}
          onSyntax={setSyntax}
          onInsert={insertVariable}
        />
      </div>

      <Show when={filtering()}>
        <Portal>
          {/* Outside the dialog, so its focus trap doesn't fight the filter dialog's. */}
          <ScriptFilterDialog
            query={draft().filter ?? defaultQuery()}
            onApply={(query) => patch({ filter: query })}
            onClose={() => setFiltering(false)}
          />
        </Portal>
      </Show>
      <Show when={runPicker()}>
        {(anchor) => (
          <GamePicker
            anchor={anchor()}
            onPick={(game) => void runNow(game)}
            onClose={() => setRunPicker(null)}
          />
        )}
      </Show>
      <Show when={showRun() && run()}>
        {(current) => (
          <ScriptRunOutput
            anchor={runAnchor()}
            run={current()}
            inside={() => [outputButton]}
            onStop={stopRun}
            onAgain={() => {
              const last = run();
              if (last) void runNow(last.game);
            }}
            onClose={(returnFocus) => {
              setShowRun(false);
              if (returnFocus) outputButton?.focus();
            }}
          />
        )}
      </Show>
      <Show when={deleting()}>
        <Portal>
          <ConfirmDialog
            title={tr("deleteTitle")}
            message={tr("deleteMessage", { name: draft().name })}
            confirmLabel={tr("delete")}
            onConfirm={() => void remove()}
            onClose={() => setDeleting(false)}
          />
        </Portal>
      </Show>
      <Show when={replacing()}>
        {(path) => (
          <Portal>
            <ConfirmDialog
              title={tr("importReplaceTitle")}
              message={tr("importReplaceMessage")}
              confirmLabel={tr("importReplace")}
              onConfirm={() => void confirmReplace(path())}
              onClose={() => setReplacing(null)}
            />
          </Portal>
        )}
      </Show>
    </Modal>
  );
}
