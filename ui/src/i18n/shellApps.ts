import type { Translations } from ".";

const en = {
  apps: "Apps",
  findApp: "Find an app",
  loading: "Loading…",
  noMatch: "No app matches.",
  noApps: "No apps yet. Mark a game as an app from its right-click menu.",
  start: "Start {name}",
  notInstalled: "{name} (not installed)",
  hint: "Click to start · right-click for more",
  showAll: "Show all apps",
};

/** The apps launcher in the title bar (AppsLauncher). */
export const shellApps: Translations<typeof en> = {
  en,
  de: {
    apps: "Apps",
    findApp: "App suchen",
    loading: "Wird geladen…",
    noMatch: "Keine App gefunden.",
    noApps: "Noch keine Apps. Markiere ein Spiel über sein Rechtsklickmenü als App.",
    start: "{name} starten",
    notInstalled: "{name} (nicht installiert)",
    hint: "Klicken zum Starten · Rechtsklick für mehr",
    showAll: "Alle Apps anzeigen",
  },
  fr: {
    apps: "Applications",
    findApp: "Trouver une application",
    loading: "Chargement…",
    noMatch: "Aucune application ne correspond.",
    noApps:
      "Aucune application pour l'instant. Marquez un jeu comme application depuis son menu contextuel.",
    start: "Lancer {name}",
    notInstalled: "{name} (non installé)",
    hint: "Clic pour lancer · clic droit pour plus",
    showAll: "Afficher toutes les applications",
  },
  it: {
    apps: "App",
    findApp: "Trova un'app",
    loading: "Caricamento…",
    noMatch: "Nessuna app corrisponde.",
    noApps: "Ancora nessuna app. Segna un gioco come app dal suo menu contestuale.",
    start: "Avvia {name}",
    notInstalled: "{name} (non installato)",
    hint: "Clic per avviare · clic destro per altro",
    showAll: "Mostra tutte le app",
  },
  es: {
    apps: "Aplicaciones",
    findApp: "Buscar una aplicación",
    loading: "Cargando…",
    noMatch: "Ninguna aplicación coincide.",
    noApps: "Aún no hay aplicaciones. Marca un juego como aplicación desde su menú contextual.",
    start: "Iniciar {name}",
    notInstalled: "{name} (no instalado)",
    hint: "Clic para iniciar · clic derecho para más",
    showAll: "Mostrar todas las aplicaciones",
  },
  pt: {
    apps: "Aplicativos",
    findApp: "Encontrar um aplicativo",
    loading: "Carregando…",
    noMatch: "Nenhum aplicativo corresponde.",
    noApps: "Ainda não há aplicativos. Marque um jogo como aplicativo no menu de contexto dele.",
    start: "Iniciar {name}",
    notInstalled: "{name} (não instalado)",
    hint: "Clique para iniciar · clique direito para mais",
    showAll: "Mostrar todos os aplicativos",
  },
};
