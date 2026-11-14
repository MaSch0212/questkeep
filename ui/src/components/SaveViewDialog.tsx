import { createSignal, For, Show } from "solid-js";
import { translator } from "../i18n";
import { libraryCommon } from "../i18n/libraryCommon";
import { librarySaveView } from "../i18n/librarySaveView";
import { joinerWord, treePills } from "../lib/filters";
import { store } from "../lib/store";
import { groupLabel, inSentence, sortLabel } from "../lib/viewOptions";
import { activeFilterCount, NO_CATEGORY, shownName } from "../lib/views";
import { IconPicker } from "./IconPicker";
import { Modal } from "./Modal";
import type { PopoverAnchor } from "./Popover";
import { sourceName } from "./SourceIcon";
import { ViewIcon } from "./ViewIcon";

const tr = translator(librarySaveView);
const trCommon = translator(libraryCommon);

const layoutName = (mode: string): string =>
  mode === "grid" ? tr("layoutGrid") : mode === "list" ? tr("layoutList") : tr("layoutDetails");

function categoryName(id: string): string {
  if (id === NO_CATEGORY) return trCommon("noCategory");
  return store.categories().find((c) => c.id === id)?.name ?? tr("deletedCategory");
}

/** "Store: Steam, GOG · Installed · Never played", or "None". */
function filterSummary(): string {
  const q = store.query();
  const parts: string[] = [];
  if (q.sources.length > 0)
    parts.push(tr("storeIs", { names: q.sources.map(sourceName).join(", ") }));
  if (q.categories.length > 0) {
    parts.push(tr("categoryIs", { names: q.categories.map(categoryName).join(", ") }));
  }
  if (q.installed !== "any")
    parts.push(q.installed === "yes" ? tr("installed") : tr("notInstalled"));
  if (q.favorite !== "any") parts.push(q.favorite === "yes" ? tr("favorites") : tr("notFavorites"));
  if (q.played !== "any") parts.push(q.played === "played" ? tr("played") : tr("neverPlayed"));
  if (q.hidden === "all") parts.push(tr("includingHidden"));
  if (q.hidden === "hidden") parts.push(tr("onlyHidden"));
  for (const pill of treePills(q.filter, store.valueName)) {
    const text = pill.parts.map((p) => (p.value ? `${p.label}: ${p.value}` : p.label));
    parts.push(pill.grouped ? `(${text.join(` ${joinerWord(pill.joiner)} `)})` : text.join(", "));
  }
  return activeFilterCount(q) === 0 ? tr("none") : parts.join(" · ");
}

/** Saves the open view's filters, sort and layout as a new view (never the keyword). */
export function SaveViewDialog(props: { onClose: () => void }) {
  const sections = store.sections();
  const current = store.activeView();
  const currentSection =
    sections.find((s) => s.views.some((v) => v.id === current?.id))?.id ?? sections[0]?.id ?? "";
  const [name, setName] = createSignal("");
  const [icon, setIcon] = createSignal("bookmark");
  const [sectionId, setSectionId] = createSignal(currentSection);
  const [picker, setPicker] = createSignal<PopoverAnchor | null>(null);
  const [saving, setSaving] = createSignal(false);
  let iconButton: HTMLButtonElement | undefined;

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (name().trim() === "" || saving()) return;
    setSaving(true);
    const ok = await store.saveAsNewView({
      name: name().trim(),
      icon: icon(),
      sectionId: sectionId(),
    });
    setSaving(false);
    if (ok) props.onClose();
  }

  return (
    <Modal title={tr("title")} onClose={props.onClose}>
      <form class="form" onSubmit={(event) => void save(event)}>
        <p class="muted">{tr("intro")}</p>
        <div class="field">
          <label class="field__label" for="view-name">
            {tr("iconAndName")}
          </label>
          <div class="field__row">
            <button
              ref={(el) => (iconButton = el)}
              type="button"
              class="icon-btn icon-btn--bordered"
              aria-label={tr("chooseIcon")}
              data-tooltip={tr("chooseIcon")}
              onClick={() => {
                const rect = iconButton?.getBoundingClientRect();
                if (rect) setPicker({ rect, align: "start" });
              }}
            >
              <ViewIcon name={icon()} />
            </button>
            <input
              id="view-name"
              class="input"
              data-autofocus
              value={name()}
              placeholder={tr("namePlaceholder")}
              onInput={(event) => setName(event.currentTarget.value)}
            />
          </div>
        </div>
        <div class="field">
          <label class="field__label" for="view-section">
            {tr("section")}
          </label>
          <select
            id="view-section"
            class="input"
            value={sectionId()}
            onChange={(event) => setSectionId(event.currentTarget.value)}
          >
            <For each={sections}>{(s) => <option value={s.id}>{shownName(s)}</option>}</For>
          </select>
        </div>
        <dl class="view-summary">
          <dt>{tr("filters")}</dt>
          <dd>{filterSummary()}</dd>
          <dt>{tr("sort")}</dt>
          <dd>
            {tr(store.query().descending ? "sortDescending" : "sortAscending", {
              sort: sortLabel(store.query().sort),
            })}
          </dd>
          <dt>{tr("layout")}</dt>
          <dd>
            {layoutName(store.viewMode())}
            {store.groupBy() === "none"
              ? ""
              : tr("groupedBy", { group: inSentence(groupLabel(store.groupBy())) })}
          </dd>
        </dl>
        <div class="form__actions">
          <button type="button" class="btn" onClick={props.onClose}>
            {tr("cancel")}
          </button>
          <button
            type="submit"
            class="btn btn--primary"
            disabled={name().trim() === "" || saving()}
          >
            {tr("saveView")}
          </button>
        </div>
      </form>
      <Show when={picker()}>
        {(anchor) => (
          <IconPicker
            anchor={anchor()}
            selected={icon()}
            onPick={setIcon}
            onClose={(returnFocus) => {
              setPicker(null);
              if (returnFocus) iconButton?.focus();
            }}
          />
        )}
      </Show>
    </Modal>
  );
}
