import type { Translations } from ".";

/** Keys are "<id>:<English default name>" of the views and sections the app creates. */
const en = {
  "all:All games": "All games",
  "favorites:Favorites": "Favorites",
  "installed:Installed": "Installed",
  "recent:Recently played": "Recently played",
  "hidden:Hidden": "Hidden",
  "library:Library": "Library",
  "other:Other": "Other",
};

/** Names of the default views and sections while the user has not renamed them. */
export const libraryDefaults: Translations<typeof en> = {
  en,
  de: {
    "all:All games": "Alle Spiele",
    "favorites:Favorites": "Favoriten",
    "installed:Installed": "Installiert",
    "recent:Recently played": "Zuletzt gespielt",
    "hidden:Hidden": "Ausgeblendet",
    "library:Library": "Bibliothek",
    "other:Other": "Sonstiges",
  },
  fr: {
    "all:All games": "Tous les jeux",
    "favorites:Favorites": "Favoris",
    "installed:Installed": "Installés",
    "recent:Recently played": "Joués récemment",
    "hidden:Hidden": "Masqués",
    "library:Library": "Bibliothèque",
    "other:Other": "Autres",
  },
  it: {
    "all:All games": "Tutti i giochi",
    "favorites:Favorites": "Preferiti",
    "installed:Installed": "Installati",
    "recent:Recently played": "Giocati di recente",
    "hidden:Hidden": "Nascosti",
    "library:Library": "Libreria",
    "other:Other": "Altro",
  },
  es: {
    "all:All games": "Todos los juegos",
    "favorites:Favorites": "Favoritos",
    "installed:Installed": "Instalados",
    "recent:Recently played": "Jugados recientemente",
    "hidden:Hidden": "Ocultos",
    "library:Library": "Biblioteca",
    "other:Other": "Otros",
  },
  pt: {
    "all:All games": "Todos os jogos",
    "favorites:Favorites": "Favoritos",
    "installed:Installed": "Instalados",
    "recent:Recently played": "Jogados recentemente",
    "hidden:Hidden": "Ocultos",
    "library:Library": "Biblioteca",
    "other:Other": "Outros",
  },
};
