// The CodeMirror 6 setup of the script editor. Loaded on demand by components/CodeEditor.tsx so
// the editor's weight is only paid when a script dialog opens.
import { autocompletion, type Completion, type CompletionContext } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { python } from "@codemirror/lang-python";
import {
  HighlightStyle,
  StreamLanguage,
  type StreamParser,
  syntaxHighlighting,
} from "@codemirror/language";
import { powerShell } from "@codemirror/legacy-modes/mode/powershell";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { EditorView, keymap, placeholder as placeholderText } from "@codemirror/view";
import { tags } from "@lezer/highlight";
import type { RuntimeId } from "../bindings/RuntimeId";
import type { ScriptVariable } from "../bindings/ScriptVariable";
import { insertionText } from "./scripts";

const BATCH_KEYWORDS = new Set(
  [
    "echo",
    "set",
    "setlocal",
    "endlocal",
    "if",
    "else",
    "for",
    "in",
    "do",
    "goto",
    "call",
    "exit",
    "start",
    "cd",
    "pushd",
    "popd",
    "copy",
    "move",
    "del",
    "mkdir",
    "rmdir",
    "type",
    "pause",
    "shift",
    "not",
    "exist",
    "defined",
    "errorlevel",
  ].map((word) => word.toLowerCase()),
);

/** Windows batch files have no CodeMirror mode: a small one for comments, %VARS% and keywords. */
const batchParser: StreamParser<unknown> = {
  name: "batch",
  token(stream) {
    if (stream.eatSpace()) return null;
    if (stream.sol() && (stream.match(/^rem(?:\s|$)/i) || stream.match(/^::/))) {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match(/^%[^%\s]+%/) || stream.match(/^%%?~?\w+/) || stream.match(/^![^!\s]+!/)) {
      return "variableName";
    }
    if (stream.match(/^"[^"]*"?/)) return "string";
    if (stream.match(/^\d+/)) return "number";
    if (stream.match(/^[A-Za-z_][\w-]*/)) {
      return BATCH_KEYWORDS.has(stream.current().toLowerCase()) ? "keyword" : null;
    }
    stream.next();
    return null;
  },
  languageData: { commentTokens: { line: "REM " } },
};

/** The language support of a runtime's scripts. */
export function languageOf(runtime: RuntimeId | null): Extension {
  switch (runtime) {
    case "bash":
      return StreamLanguage.define(shell);
    case "cmd":
      return StreamLanguage.define(batchParser);
    case "python":
      return python();
    case "pwsh":
    case "windowsPowershell":
    default:
      return StreamLanguage.define(powerShell);
  }
}

const KEYWORDS: Record<RuntimeId, string[]> = {
  pwsh: [
    "if",
    "else",
    "elseif",
    "foreach",
    "for",
    "while",
    "switch",
    "function",
    "param",
    "return",
    "try",
    "catch",
    "finally",
    "throw",
    "Write-Output",
    "Write-Host",
    "Start-Process",
    "Stop-Process",
    "Get-Process",
    "Set-Location",
    "Test-Path",
    "Copy-Item",
    "Remove-Item",
    "Invoke-WebRequest",
  ],
  windowsPowershell: [],
  bash: [
    "if",
    "then",
    "else",
    "elif",
    "fi",
    "for",
    "while",
    "do",
    "done",
    "case",
    "esac",
    "function",
    "echo",
    "export",
    "exit",
    "cd",
    "mkdir",
    "cp",
    "mv",
    "rm",
    "test",
  ],
  cmd: [
    "echo",
    "set",
    "setlocal",
    "if",
    "else",
    "for",
    "goto",
    "call",
    "exit",
    "start",
    "cd",
    "copy",
    "move",
    "del",
    "mkdir",
    "pause",
    "rem",
  ],
  python: [
    "import",
    "from",
    "def",
    "class",
    "if",
    "elif",
    "else",
    "for",
    "while",
    "try",
    "except",
    "finally",
    "with",
    "return",
    "print",
    "os",
    "sys",
    "subprocess",
    "pathlib",
  ],
};
KEYWORDS.windowsPowershell = KEYWORDS.pwsh;

/** The completions of a runtime: its keywords and the variables in its own syntax. */
export function completionsFor(
  runtime: RuntimeId | null,
  variables: readonly ScriptVariable[],
): Completion[] {
  const keywords = (runtime ? KEYWORDS[runtime] : KEYWORDS.pwsh).map((label): Completion => ({
    label,
    type: "keyword",
  }));
  const vars = variables.map((variable): Completion => ({
    label: insertionText(variable, "env", runtime, "editor"),
    detail: variable.value.length > 28 ? `${variable.value.slice(0, 27)}…` : variable.value,
    type: "variable",
    boost: 1,
  }));
  return [...vars, ...keywords];
}

// The colors come from the app's theme variables, so the editor follows every theme and scheme.
const theme = EditorView.theme({
  "&": {
    color: "var(--qk-color-text)",
    backgroundColor: "var(--qk-color-surface)",
    fontSize: "var(--qk-font-size-sm)",
    border: "1px solid var(--qk-color-border)",
    borderRadius: "var(--qk-radius-sm)",
  },
  "&.cm-focused": { outline: "var(--qk-focus-ring-width) solid var(--qk-color-accent)" },
  ".cm-scroller": { fontFamily: "var(--qk-font-mono)", lineHeight: "var(--qk-line-height)" },
  ".cm-content": { caretColor: "var(--qk-color-text)", padding: "var(--qk-space-2) 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--qk-color-text)" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
    { backgroundColor: "var(--qk-color-accent-soft)" },
  ".cm-gutters": {
    backgroundColor: "var(--qk-color-surface-raised)",
    color: "var(--qk-color-text-faint)",
    border: "none",
  },
  ".cm-placeholder": { color: "var(--qk-color-text-faint)" },
  ".cm-tooltip": {
    backgroundColor: "var(--qk-color-surface-raised)",
    border: "1px solid var(--qk-color-border)",
    borderRadius: "var(--qk-radius-sm)",
    boxShadow: "var(--qk-shadow-popover)",
    color: "var(--qk-color-text)",
  },
  ".cm-tooltip-autocomplete ul li[aria-selected]": {
    backgroundColor: "var(--qk-color-accent-soft)",
    color: "var(--qk-color-text)",
  },
  ".cm-completionDetail": { color: "var(--qk-color-text-muted)", marginLeft: "var(--qk-space-2)" },
});

const highlight = HighlightStyle.define([
  {
    tag: [tags.keyword, tags.operatorKeyword, tags.controlKeyword],
    color: "var(--qk-color-accent)",
  },
  { tag: [tags.string, tags.special(tags.string)], color: "var(--qk-color-success)" },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment],
    color: "var(--qk-color-text-faint)",
    fontStyle: "italic",
  },
  { tag: [tags.number, tags.bool, tags.atom], color: "var(--qk-color-warn)" },
  { tag: [tags.variableName, tags.special(tags.variableName)], color: "var(--qk-color-play)" },
  {
    tag: [tags.function(tags.variableName), tags.definition(tags.variableName)],
    color: "var(--qk-color-text)",
  },
]);

export interface CodeEditorOptions {
  parent: HTMLElement;
  doc: string;
  runtime: RuntimeId | null;
  /** The variables to offer (read at each completion, so changes apply without a rebuild). */
  variables: () => readonly ScriptVariable[];
  placeholder?: string;
  label: string;
  onChange: (text: string) => void;
  onFocus: () => void;
}

export interface CodeEditorHandle {
  setDoc: (text: string) => void;
  setRuntime: (runtime: RuntimeId | null) => void;
  /** Inserts text at the cursor (replacing the selection) and keeps the focus in the editor. */
  insert: (text: string) => void;
  focus: () => void;
  destroy: () => void;
}

export function createCodeEditor(options: CodeEditorOptions): CodeEditorHandle {
  const language = new Compartment();
  let runtime = options.runtime;

  const completionSource = (context: CompletionContext) => {
    const word = context.matchBefore(/[\w$%:.[\]"-]+/);
    if (!word && !context.explicit) return null;
    if (word && word.text.length < 2 && !context.explicit) return null;
    return {
      from: word ? word.from : context.pos,
      options: completionsFor(runtime, options.variables()),
      validFor: /^[\w$%:.[\]"-]*$/,
    };
  };

  const view = new EditorView({
    parent: options.parent,
    state: EditorState.create({
      doc: options.doc,
      extensions: [
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        language.of(languageOf(runtime)),
        syntaxHighlighting(highlight),
        theme,
        autocompletion({ override: [completionSource] }),
        EditorView.lineWrapping,
        EditorView.contentAttributes.of({ "aria-label": options.label }),
        ...(options.placeholder ? [placeholderText(options.placeholder)] : []),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) options.onChange(update.state.doc.toString());
        }),
        EditorView.domEventHandlers({
          focus: () => {
            options.onFocus();
            return false;
          },
        }),
      ],
    }),
  });

  return {
    setDoc(text) {
      if (text === view.state.doc.toString()) return;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
    },
    setRuntime(next) {
      runtime = next;
      view.dispatch({ effects: language.reconfigure(languageOf(next)) });
    },
    insert(text) {
      view.dispatch(view.state.replaceSelection(text), { scrollIntoView: true });
      view.focus();
    },
    focus: () => view.focus(),
    destroy: () => view.destroy(),
  };
}
