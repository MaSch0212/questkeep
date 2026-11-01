import type { Translations } from ".";

const en = {
  games: "Games",
  empty: "No games to show. Add a game, or scan a store from Settings.",
  notInstalledName: "{name} (not installed)",
  favorite: "Favorite",
  hidden: "Hidden",
  running: "Running",
  notInstalled: "Not installed",
  installed: "Installed",
};

/** Game grid, list, rows and tiles (GameGrid, GameList, GameRow, GameRowCompact, GameTile). */
export const libraryList: Translations<typeof en> = {
  en,
  de: {
    games: "Spiele",
    empty:
      "Keine Spiele vorhanden. Füge ein Spiel hinzu oder durchsuche in den Einstellungen einen Store.",
    notInstalledName: "{name} (nicht installiert)",
    favorite: "Favorit",
    hidden: "Ausgeblendet",
    running: "Läuft",
    notInstalled: "Nicht installiert",
    installed: "Installiert",
  },
  fr: {
    games: "Jeux",
    empty: "Aucun jeu à afficher. Ajoutez un jeu ou analysez une boutique depuis les Paramètres.",
    notInstalledName: "{name} (non installé)",
    favorite: "Favori",
    hidden: "Masqué",
    running: "En cours",
    notInstalled: "Non installé",
    installed: "Installé",
  },
  it: {
    games: "Giochi",
    empty: "Nessun gioco da mostrare. Aggiungi un gioco o analizza uno store dalle Impostazioni.",
    notInstalledName: "{name} (non installato)",
    favorite: "Preferito",
    hidden: "Nascosto",
    running: "In esecuzione",
    notInstalled: "Non installato",
    installed: "Installato",
  },
  es: {
    games: "Juegos",
    empty: "No hay juegos que mostrar. Añade un juego o analiza una tienda desde Configuración.",
    notInstalledName: "{name} (no instalado)",
    favorite: "Favorito",
    hidden: "Oculto",
    running: "En ejecución",
    notInstalled: "No instalado",
    installed: "Instalado",
  },
  pt: {
    games: "Jogos",
    empty: "Nenhum jogo para mostrar. Adicione um jogo ou verifique uma loja nas Configurações.",
    notInstalledName: "{name} (não instalado)",
    favorite: "Favorito",
    hidden: "Oculto",
    running: "Em execução",
    notInstalled: "Não instalado",
    installed: "Instalado",
  },
};
