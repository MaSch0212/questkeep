import type { Translations } from ".";

const en = {
  gamesCount: (p: { count: number }) => (p.count === 1 ? "1 game" : `${p.count} games`),
  noCategory: "No category",
  nothingFound: "Nothing found",
};

/** Small texts shared by the library browsing components. */
export const libraryCommon: Translations<typeof en> = {
  en,
  de: {
    gamesCount: (p) => (p.count === 1 ? "1 Spiel" : `${p.count} Spiele`),
    noCategory: "Keine Kategorie",
    nothingFound: "Nichts gefunden",
  },
  fr: {
    gamesCount: (p) => (p.count < 2 ? `${p.count} jeu` : `${p.count} jeux`),
    noCategory: "Aucune catégorie",
    nothingFound: "Aucun résultat",
  },
  it: {
    gamesCount: (p) => (p.count === 1 ? "1 gioco" : `${p.count} giochi`),
    noCategory: "Nessuna categoria",
    nothingFound: "Nessun risultato",
  },
  es: {
    gamesCount: (p) => (p.count === 1 ? "1 juego" : `${p.count} juegos`),
    noCategory: "Sin categoría",
    nothingFound: "Sin resultados",
  },
  pt: {
    gamesCount: (p) => (p.count === 1 ? "1 jogo" : `${p.count} jogos`),
    noCategory: "Sem categoria",
    nothingFound: "Nada encontrado",
  },
};
