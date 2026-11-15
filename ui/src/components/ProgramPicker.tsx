import { createMemo, createResource, createSignal, For, Match, Switch } from "solid-js";
import type { InstalledProgram } from "../bindings/InstalledProgram";
import { translator } from "../i18n";
import { gameSetup } from "../i18n/gameSetup";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { Modal } from "./Modal";

const tr = translator(gameSetup);

/** Programs on this PC that are not in the library yet; picking one fills in Add game. */
export function ProgramPicker(props: {
  onPick: (program: InstalledProgram) => void;
  onClose: () => void;
}) {
  const [programs] = createResource(() =>
    Promise.resolve()
      .then(() => api.installedPrograms())
      .then(
        (list) => ({ list, error: null }),
        (failure: unknown) => ({ list: [] as InstalledProgram[], error: errorMessage(failure) }),
      ),
  );
  const [filter, setFilter] = createSignal("");
  const shown = createMemo(() => {
    const words = filter().toLocaleLowerCase().split(/\s+/).filter(Boolean);
    return (programs()?.list ?? []).filter((program) => {
      const text = `${program.name} ${program.executable}`.toLocaleLowerCase();
      return words.every((word) => text.includes(word));
    });
  });

  return (
    <Modal title={tr("programsTitle")} tall onClose={props.onClose}>
      <div class="program-picker">
        <input
          class="input"
          type="search"
          data-autofocus
          aria-label={tr("filterPrograms")}
          placeholder={tr("filterPrograms")}
          value={filter()}
          onInput={(event) => setFilter(event.currentTarget.value)}
          onKeyDown={(event) => {
            const first = shown()[0];
            if (event.key === "Enter" && first) {
              event.preventDefault();
              props.onPick(first);
            }
          }}
        />
        <Switch>
          <Match when={programs.loading}>
            <p class="muted">{tr("loadingPrograms")}</p>
          </Match>
          <Match when={programs()?.error}>
            <p class="error-text" role="alert">
              {programs()?.error}
            </p>
          </Match>
          <Match when={programs()?.list.length === 0}>
            <p class="muted">{tr("noPrograms")}</p>
          </Match>
          <Match when={shown().length === 0}>
            <p class="muted">{tr("noProgramMatch")}</p>
          </Match>
          <Match when={true}>
            <ul class="program-picker__list" aria-label={tr("programsTitle")}>
              <For each={shown()}>
                {(program) => (
                  <li>
                    <button
                      type="button"
                      class="program-picker__item"
                      onClick={() => props.onPick(program)}
                    >
                      <span class="program-picker__name">{program.name}</span>
                      <small class="muted">
                        {program.origin === "startMenu" ? tr("fromStartMenu") : tr("fromApps")} ·{" "}
                        {program.executable}
                      </small>
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Match>
        </Switch>
      </div>
    </Modal>
  );
}
