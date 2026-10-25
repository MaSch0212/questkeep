import { createSignal, For, Show } from "solid-js";
import { store } from "../lib/store";
import { activeFilterCount, NO_CATEGORY } from "../lib/views";
import { IconPicker } from "./IconPicker";
import { Modal } from "./Modal";
import type { PopoverAnchor } from "./Popover";
import { sourceName } from "./SourceIcon";
import { ViewIcon } from "./ViewIcon";

const SORT_NAMES: Record<string, string> = {
  name: "Name",
  lastPlayed: "Last played",
  playtime: "Playtime",
  dateAdded: "Date added",
};

const LAYOUT_NAMES: Record<string, string> = { grid: "Grid", list: "List", details: "Details" };
const GROUP_NAMES: Record<string, string> = { category: "category", source: "store" };

function categoryName(id: string): string {
  if (id === NO_CATEGORY) return "No category";
  return store.categories().find((c) => c.id === id)?.name ?? "Deleted category";
}

/** "Store: Steam, GOG · Installed · Never played", or "None". */
function filterSummary(): string {
  const q = store.query();
  const parts: string[] = [];
  if (q.sources.length > 0) parts.push(`Store: ${q.sources.map(sourceName).join(", ")}`);
  if (q.categories.length > 0) {
    parts.push(`Category: ${q.categories.map(categoryName).join(", ")}`);
  }
  if (q.installed !== "any") parts.push(q.installed === "yes" ? "Installed" : "Not installed");
  if (q.favorite !== "any") parts.push(q.favorite === "yes" ? "Favorites" : "Not favorites");
  if (q.played !== "any") parts.push(q.played === "played" ? "Played" : "Never played");
  if (q.hidden === "all") parts.push("Including hidden");
  if (q.hidden === "hidden") parts.push("Only hidden");
  return activeFilterCount(q) === 0 ? "None" : parts.join(" · ");
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
    <Modal title="Save as new view" onClose={props.onClose}>
      <form class="form" onSubmit={(event) => void save(event)}>
        <p class="muted">Saves the filters, sort order and layout. The filter box is not saved.</p>
        <div class="field">
          <label class="field__label" for="view-name">
            Icon and name
          </label>
          <div class="field__row">
            <button
              ref={(el) => (iconButton = el)}
              type="button"
              class="icon-btn icon-btn--bordered"
              aria-label="Choose an icon"
              title="Choose an icon"
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
              placeholder="My view"
              onInput={(event) => setName(event.currentTarget.value)}
            />
          </div>
        </div>
        <div class="field">
          <label class="field__label" for="view-section">
            Section
          </label>
          <select
            id="view-section"
            class="input"
            value={sectionId()}
            onChange={(event) => setSectionId(event.currentTarget.value)}
          >
            <For each={sections}>{(s) => <option value={s.id}>{s.name}</option>}</For>
          </select>
        </div>
        <dl class="view-summary">
          <dt>Filters</dt>
          <dd>{filterSummary()}</dd>
          <dt>Sort</dt>
          <dd>
            {SORT_NAMES[store.query().sort]},{" "}
            {store.query().descending ? "descending" : "ascending"}
          </dd>
          <dt>Layout</dt>
          <dd>
            {LAYOUT_NAMES[store.viewMode()]}
            {store.groupBy() === "none" ? "" : `, grouped by ${GROUP_NAMES[store.groupBy()]}`}
          </dd>
        </dl>
        <div class="form__actions">
          <button type="button" class="btn" onClick={props.onClose}>
            Cancel
          </button>
          <button
            type="submit"
            class="btn btn--primary"
            disabled={name().trim() === "" || saving()}
          >
            Save view
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
