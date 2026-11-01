import type { Translations } from ".";

const en = {
  title: "Games in several stores",
  looking: "Looking for games in several stores…",
  noneFound: "No games found that are in more than one store.",
  intro:
    "These look like the same game in different stores. Combined, they show as one game with every store, and Play starts the installed copy. You can separate them again on the game's page.",
  differentAria: "{name} are different games",
  notSame: "Not the same",
  combineAria: "Combine {name}",
  combine: "Combine",
  close: "Close",
  combineAll: "Combine all ({count})",
};

/** Games found in several stores (DuplicatesDialog). */
export const shellDuplicates: Translations<typeof en> = {
  en,
  de: {
    title: "Spiele in mehreren Stores",
    looking: "Es wird nach Spielen in mehreren Stores gesucht…",
    noneFound: "Keine Spiele gefunden, die in mehr als einem Store sind.",
    intro:
      "Das scheinen dieselben Spiele in verschiedenen Stores zu sein. Zusammengeführt erscheinen sie als ein Spiel mit allen Stores, und „Spielen“ startet die installierte Kopie. Auf der Spielseite kannst du sie wieder trennen.",
    differentAria: "{name} sind verschiedene Spiele",
    notSame: "Nicht dasselbe",
    combineAria: "{name} zusammenführen",
    combine: "Zusammenführen",
    close: "Schließen",
    combineAll: "Alle zusammenführen ({count})",
  },
  fr: {
    title: "Jeux dans plusieurs boutiques",
    looking: "Recherche des jeux présents dans plusieurs boutiques…",
    noneFound: "Aucun jeu trouvé qui soit dans plusieurs boutiques.",
    intro:
      "Il semble s'agir du même jeu dans différentes boutiques. Une fois combinés, ils apparaissent comme un seul jeu avec toutes les boutiques, et Jouer lance la copie installée. Vous pouvez les séparer à nouveau sur la page du jeu.",
    differentAria: "{name} sont des jeux différents",
    notSame: "Pas le même",
    combineAria: "Combiner {name}",
    combine: "Combiner",
    close: "Fermer",
    combineAll: "Tout combiner ({count})",
  },
  it: {
    title: "Giochi in più store",
    looking: "Ricerca di giochi presenti in più store…",
    noneFound: "Nessun gioco trovato presente in più di uno store.",
    intro:
      "Sembrano lo stesso gioco in store diversi. Una volta uniti, compaiono come un unico gioco con tutti gli store e Gioca avvia la copia installata. Puoi separarli di nuovo dalla pagina del gioco.",
    differentAria: "{name} sono giochi diversi",
    notSame: "Non sono uguali",
    combineAria: "Unisci {name}",
    combine: "Unisci",
    close: "Chiudi",
    combineAll: "Unisci tutti ({count})",
  },
  es: {
    title: "Juegos en varias tiendas",
    looking: "Buscando juegos que estén en varias tiendas…",
    noneFound: "No se encontraron juegos que estén en más de una tienda.",
    intro:
      "Parecen ser el mismo juego en tiendas distintas. Combinados, aparecen como un solo juego con todas las tiendas y Jugar inicia la copia instalada. Puedes separarlos de nuevo en la página del juego.",
    differentAria: "{name} son juegos distintos",
    notSame: "No es el mismo",
    combineAria: "Combinar {name}",
    combine: "Combinar",
    close: "Cerrar",
    combineAll: "Combinar todos ({count})",
  },
  pt: {
    title: "Jogos em várias lojas",
    looking: "Procurando jogos que estejam em várias lojas…",
    noneFound: "Nenhum jogo encontrado que esteja em mais de uma loja.",
    intro:
      "Parecem ser o mesmo jogo em lojas diferentes. Combinados, aparecem como um só jogo com todas as lojas, e Jogar inicia a cópia instalada. Você pode separá-los de novo na página do jogo.",
    differentAria: "{name} são jogos diferentes",
    notSame: "Não é o mesmo",
    combineAria: "Combinar {name}",
    combine: "Combinar",
    close: "Fechar",
    combineAll: "Combinar todos ({count})",
  },
};
