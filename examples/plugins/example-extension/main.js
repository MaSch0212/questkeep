// @ts-check
// An example Questkeep extension in plain JavaScript: no build step, types through JSDoc.
// Everything registered through `api` is removed again when the plugin is unloaded or reloaded.

/** @typedef {import("../questkeep-plugin").QuestkeepApi} QuestkeepApi */
/** @typedef {import("../questkeep-plugin").Game} Game */

const ICON_DICE =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="3"/><circle cx="8.5" cy="8.5" r="1" fill="currentColor"/><circle cx="15.5" cy="15.5" r="1" fill="currentColor"/><circle cx="15.5" cy="8.5" r="1" fill="currentColor"/><circle cx="8.5" cy="15.5" r="1" fill="currentColor"/></svg>';
const ICON_CHART =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20V10"/><path d="M10 20V4"/><path d="M16 20v-7"/><path d="M22 20H2"/></svg>';
const ICON_COPY =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/></svg>';

/**
 * A small DOM helper: `h("button", { class: "btn", onclick: fn }, "Label")`.
 * @param {string} tag
 * @param {Record<string, unknown>} [props]
 * @param {...(Node | string)} children
 */
function h(tag, props = {}, ...children) {
  const element = document.createElement(tag);
  for (const [name, value] of Object.entries(props)) {
    if (name.startsWith("on") && typeof value === "function") {
      element.addEventListener(name.slice(2), /** @type {EventListener} */ (value));
    } else if (value !== undefined && value !== null && value !== false) {
      element.setAttribute(name, value === true ? "" : String(value));
    }
  }
  element.append(...children);
  return element;
}

/** @param {number} seconds */
function hours(seconds) {
  return `${Math.round(seconds / 3600).toLocaleString()} h`;
}

/**
 * The games "Surprise me" picks from, by the `pool` setting.
 * @param {Game[]} games
 * @param {unknown} pool
 */
function candidates(games, pool) {
  const real = games.filter((game) => game.kind === "game" && !game.hidden);
  if (pool === "all") return real;
  const installed = real.filter((game) => game.installed);
  return pool === "unplayed" ? installed.filter((game) => game.playtimeSeconds === 0) : installed;
}

/** @param {QuestkeepApi} api */
function surpriseMe(api) {
  api.ui.showDialog({
    title: "Surprise me",
    width: "small",
    render(container, close) {
      const name = h("p", { class: "example-surprise__name" }, "…");
      const detail = h("p", { class: "muted" });
      const open = h("button", { type: "button", class: "btn btn--primary" }, "Open game page");
      const again = h("button", { type: "button", class: "btn" }, "Another one");
      /** @type {Game | undefined} */
      let picked;
      let closed = false;

      async function roll() {
        const pool = candidates(await api.games.list(), api.settings.get("pool"));
        if (closed) return;
        picked = pool[Math.floor(Math.random() * pool.length)];
        name.textContent = picked ? picked.name : "Nothing to pick from";
        detail.textContent = picked
          ? `${picked.source} · played ${hours(picked.playtimeSeconds)}`
          : "No game matches the setting.";
        open.toggleAttribute("disabled", !picked);
      }

      // Shows the game's page. An extension could start a game with
      // api.invoke("launch_game", …); this example deliberately never does.
      open.addEventListener("click", () => {
        if (!picked) return;
        api.games.open(picked.id);
        close();
      });
      again.addEventListener("click", () => void roll());

      container.append(
        h(
          "div",
          { class: "example-surprise" },
          name,
          detail,
          h("div", { class: "example-surprise__actions" }, again, open),
        ),
      );
      void roll();
      return () => {
        closed = true;
      };
    },
  });
}

/**
 * The "Library facts" page: numbers about the library, kept up to date.
 * @param {QuestkeepApi} api
 * @param {HTMLElement} container
 */
function renderFacts(api, container) {
  const root = h("div", { class: "example-facts" }, h("p", { class: "muted" }, "Loading…"));
  container.append(root);
  let gone = false;

  async function update() {
    const games = (await api.games.list()).filter((game) => game.kind === "game");
    if (gone) return;
    const playtime = games.reduce((sum, game) => sum + game.playtimeSeconds, 0);
    /** @type {Map<string, number>} */
    const perStore = new Map();
    for (const game of games) perStore.set(game.source, (perStore.get(game.source) ?? 0) + 1);
    const top = [...perStore].sort((a, b) => b[1] - a[1]).slice(0, api.settings.get("topStores", 5));
    const highlight = api.settings.get("highlight", "");

    /** @param {string} label @param {string} value */
    const card = (label, value) =>
      h(
        "div",
        { class: "example-facts__card" },
        h("span", { class: "example-facts__value", style: highlight && `color: ${highlight}` }, value),
        h("span", { class: "muted" }, label),
      );

    root.replaceChildren(
      h(
        "div",
        { class: "example-facts__numbers" },
        card("games", games.length.toLocaleString()),
        card("installed", games.filter((game) => game.installed).length.toLocaleString()),
        card("played in total", hours(playtime)),
      ),
      h(
        "ul",
        { class: "example-facts__stores", "aria-label": "Games per store" },
        ...top.map(([store, count]) =>
          h("li", { class: "example-facts__store" }, h("span", {}, store), h("b", {}, String(count))),
        ),
      ),
    );
  }

  // The page follows the library and its own settings while it is open.
  const listeners = [
    api.events.on("gamesChanged", () => void update()),
    api.events.on("gameRemoved", () => void update()),
    api.settings.onChange(() => void update()),
  ];
  void update();
  return () => {
    gone = true;
    for (const listener of listeners) listener.dispose();
  };
}

/**
 * Own UI in the plugin's settings: a preview that follows the settings above it.
 * @param {QuestkeepApi} api
 * @param {HTMLElement} container
 */
function renderPreview(api, container) {
  const swatch = h("span", { class: "example-preview__swatch" });
  const text = h("span");
  const reset = h("button", { type: "button", class: "btn btn--small" }, "Reset the highlight");
  reset.addEventListener("click", () => void api.settings.set("highlight", undefined));
  container.append(h("div", { class: "example-preview" }, swatch, text, reset));

  function update() {
    const { highlight, pool, topStores } = api.settings.all();
    swatch.style.background = String(highlight);
    text.textContent = `Picks from "${pool}", lists ${topStores} stores, highlights in ${highlight}.`;
  }
  update();
  const listener = api.settings.onChange(update);
  return () => listener.dispose();
}

/** @type {import("../questkeep-plugin").ExtensionModule["activate"]} */
export function activate(api) {
  api.ui.addTitleBarButton({
    id: "surprise",
    label: "Surprise me",
    icon: ICON_DICE,
    onClick: () => surpriseMe(api),
  });

  const facts = api.ui.addPage({
    id: "facts",
    title: "Library facts",
    icon: ICON_CHART,
    render: (container) => renderFacts(api, container),
  });

  api.ui.addMenuItems("app", () => [
    { label: "Library facts", icon: ICON_CHART, onSelect: () => facts.open() },
  ]);

  api.ui.addMenuItems("game", ({ game }) => {
    if (!game) return [];
    /** @param {string} text @param {string} what */
    const copy = (text, what) =>
      navigator.clipboard.writeText(text).then(
        () => api.ui.toast(`Copied the ${what}.`),
        () => api.ui.toast("Copying is not allowed here.", "error"),
      );
    return [
      { label: "Copy name", icon: ICON_COPY, onSelect: () => void copy(game.name, "name") },
      {
        label: "Example extension",
        items: [
          { label: "Copy Questkeep id", onSelect: () => void copy(game.id, "id") },
          {
            label: "Copy store id",
            disabled: game.sourceGameId === null,
            onSelect: () => void copy(game.sourceGameId ?? "", "store id"),
          },
          { separator: true },
          { label: "Surprise me instead", icon: ICON_DICE, onSelect: () => surpriseMe(api) },
        ],
      },
    ];
  });

  api.ui.addSettingsSection({
    id: "preview",
    title: "Preview",
    render: (container) => renderPreview(api, container),
  });

  if (api.settings.get("greet", true)) {
    api.ui.toast(`${api.plugin.name} ${api.plugin.version} is running.`);
  }
  api.log.info("activated");
}

export function deactivate() {
  // Nothing to do: everything above was registered through `api` and is removed by the app.
}
