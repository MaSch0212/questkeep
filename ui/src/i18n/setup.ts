import type { Translations } from ".";

const en = {
  title: "Set up Questkeep",
  step: "Step {n} of 3",
  welcomeTitle: "Welcome to Questkeep",
  welcomeText:
    "All your PC games from every store in one library. Questkeep already looks for installed stores; the next two steps let you sign in to stores and set up optional keys for richer game details. Everything here can be changed later in Settings.",
  storesTitle: "Your stores",
  keysTitle: "Richer game details (optional)",
  keysIntro:
    "Questkeep gets most details from your stores. Two free services add more; both need a key from your own free account, stored only in your system's credential store.",
  igdbTitle: "IGDB (via Twitch)",
  recommended: "Recommended",
  igdbDesc:
    "Time to beat, wiki and social links, series, age ratings, game modes, themes, and details for games your stores know little about.",
  optional: "Optional",
  griddbDesc:
    "Covers for games without one, and more covers, backgrounds and icons in the image picker.",
  warnTitle: "Continue without IGDB?",
  warnStrong: "Without an IGDB key these stay empty:",
  without1: "Time to beat (only HowLongToBeat's numbers remain, when it finds the game)",
  without2: "Website, wiki and social links (store links still work)",
  without3: "Series, age ratings, game modes, themes, perspectives, and filters on them",
  without4:
    "Details for games from stores without metadata (Ubisoft, EA, Battle.net, added by hand)",
  warnHint:
    'You can add the key any time in Settings → Metadata, then use "Refresh metadata for all games".',
  skipSetup: "Skip setup",
  getStarted: "Get started",
  back: "Back",
  next: "Next",
  skip: "Skip",
  finish: "Finish",
  saving: "Saving…",
  backToKeys: "Back to keys",
  continueWithout: "Continue without",
};

/** The first-run setup (FirstRunDialog). */
export const setup: Translations<typeof en> = {
  en,
  de: {
    title: "Questkeep einrichten",
    step: "Schritt {n} von 3",
    welcomeTitle: "Willkommen bei Questkeep",
    welcomeText:
      "Alle deine PC-Spiele aus allen Stores in einer Bibliothek. Questkeep sucht bereits nach installierten Stores; in den nächsten zwei Schritten kannst du dich bei Stores anmelden und optionale Schlüssel für ausführlichere Spieldetails einrichten. Alles hier lässt sich später in den Einstellungen ändern.",
    storesTitle: "Deine Stores",
    keysTitle: "Ausführlichere Spieldetails (optional)",
    keysIntro:
      "Questkeep bekommt die meisten Details von deinen Stores. Zwei kostenlose Dienste liefern mehr; beide brauchen einen Schlüssel aus deinem eigenen kostenlosen Konto, der nur im Anmeldeinformationsspeicher deines Systems abgelegt wird.",
    igdbTitle: "IGDB (über Twitch)",
    recommended: "Empfohlen",
    igdbDesc:
      "Durchspielzeit, Wiki- und Social-Media-Links, Serien, Altersfreigaben, Spielmodi, Themen und Details zu Spielen, über die deine Stores wenig wissen.",
    optional: "Optional",
    griddbDesc:
      "Cover für Spiele ohne eines sowie mehr Cover, Hintergründe und Symbole in der Bildauswahl.",
    warnTitle: "Ohne IGDB fortfahren?",
    warnStrong: "Ohne IGDB-Schlüssel bleiben diese Angaben leer:",
    without1: "Durchspielzeit (nur die Werte von HowLongToBeat bleiben, wenn es das Spiel findet)",
    without2: "Website-, Wiki- und Social-Media-Links (Store-Links funktionieren weiterhin)",
    without3: "Serien, Altersfreigaben, Spielmodi, Themen, Perspektiven und Filter dafür",
    without4:
      "Details zu Spielen aus Stores ohne Metadaten (Ubisoft, EA, Battle.net, von Hand hinzugefügt)",
    warnHint:
      "Du kannst den Schlüssel jederzeit unter Einstellungen → Metadaten hinzufügen und dann „Metadaten aller Spiele aktualisieren“ verwenden.",
    skipSetup: "Einrichtung überspringen",
    getStarted: "Los geht’s",
    back: "Zurück",
    next: "Weiter",
    skip: "Überspringen",
    finish: "Fertig",
    saving: "Wird gespeichert…",
    backToKeys: "Zurück zu den Schlüsseln",
    continueWithout: "Ohne fortfahren",
  },
  fr: {
    title: "Configurer Questkeep",
    step: "Étape {n} sur 3",
    welcomeTitle: "Bienvenue dans Questkeep",
    welcomeText:
      "Tous vos jeux PC de toutes les boutiques dans une seule bibliothèque. Questkeep recherche déjà les boutiques installées ; les deux prochaines étapes vous permettent de vous connecter aux boutiques et de configurer des clés facultatives pour des détails de jeu plus riches. Tout cela peut être modifié plus tard dans les paramètres.",
    storesTitle: "Vos boutiques",
    keysTitle: "Des détails de jeu plus riches (facultatif)",
    keysIntro:
      "Questkeep obtient la plupart des détails auprès de vos boutiques. Deux services gratuits en ajoutent ; tous deux demandent une clé issue de votre propre compte gratuit, conservée uniquement dans le gestionnaire d'identifiants de votre système.",
    igdbTitle: "IGDB (via Twitch)",
    recommended: "Recommandé",
    igdbDesc:
      "Durée de vie, liens wiki et réseaux sociaux, séries, classifications d'âge, modes de jeu, thèmes, et détails des jeux que vos boutiques connaissent mal.",
    optional: "Facultatif",
    griddbDesc:
      "Des jaquettes pour les jeux qui n'en ont pas, et plus de jaquettes, d'arrière-plans et d'icônes dans le sélecteur d'images.",
    warnTitle: "Continuer sans IGDB ?",
    warnStrong: "Sans clé IGDB, ces informations resteront vides :",
    without1: "Durée de vie (seuls les chiffres de HowLongToBeat restent, quand il trouve le jeu)",
    without2:
      "Liens vers le site web, le wiki et les réseaux sociaux (les liens des boutiques fonctionnent toujours)",
    without3:
      "Séries, classifications d'âge, modes de jeu, thèmes, perspectives, et les filtres correspondants",
    without4:
      "Détails des jeux issus de boutiques sans métadonnées (Ubisoft, EA, Battle.net, ajoutés à la main)",
    warnHint:
      "Vous pouvez ajouter la clé à tout moment dans Paramètres → Métadonnées, puis utiliser « Actualiser les métadonnées de tous les jeux ».",
    skipSetup: "Ignorer la configuration",
    getStarted: "Commencer",
    back: "Retour",
    next: "Suivant",
    skip: "Ignorer",
    finish: "Terminer",
    saving: "Enregistrement…",
    backToKeys: "Retour aux clés",
    continueWithout: "Continuer sans",
  },
  it: {
    title: "Configura Questkeep",
    step: "Passo {n} di 3",
    welcomeTitle: "Benvenuto in Questkeep",
    welcomeText:
      "Tutti i tuoi giochi per PC di ogni store in un'unica libreria. Questkeep cerca già gli store installati; i prossimi due passi ti permettono di accedere agli store e di configurare chiavi facoltative per dettagli di gioco più ricchi. Tutto questo si può cambiare in seguito nelle impostazioni.",
    storesTitle: "I tuoi store",
    keysTitle: "Dettagli di gioco più ricchi (facoltativo)",
    keysIntro:
      "Questkeep ricava la maggior parte dei dettagli dai tuoi store. Due servizi gratuiti ne aggiungono altri; entrambi richiedono una chiave del tuo account gratuito, salvata solo nell'archivio delle credenziali del sistema.",
    igdbTitle: "IGDB (tramite Twitch)",
    recommended: "Consigliato",
    igdbDesc:
      "Tempo per finire, link a wiki e social, serie, classificazioni per età, modalità di gioco, temi e dettagli per i giochi che i tuoi store conoscono poco.",
    optional: "Facoltativo",
    griddbDesc:
      "Copertine per i giochi che non ne hanno, e più copertine, sfondi e icone nel selettore di immagini.",
    warnTitle: "Continuare senza IGDB?",
    warnStrong: "Senza una chiave IGDB queste informazioni restano vuote:",
    without1: "Tempo per finire (restano solo i dati di HowLongToBeat, se trova il gioco)",
    without2: "Link a sito web, wiki e social (i link degli store funzionano ancora)",
    without3:
      "Serie, classificazioni per età, modalità di gioco, temi, prospettive e relativi filtri",
    without4:
      "Dettagli per i giochi di store senza metadati (Ubisoft, EA, Battle.net, aggiunti a mano)",
    warnHint:
      "Puoi aggiungere la chiave in qualsiasi momento in Impostazioni → Metadati, poi usare “Aggiorna i metadati di tutti i giochi”.",
    skipSetup: "Salta la configurazione",
    getStarted: "Inizia",
    back: "Indietro",
    next: "Avanti",
    skip: "Salta",
    finish: "Fine",
    saving: "Salvataggio…",
    backToKeys: "Torna alle chiavi",
    continueWithout: "Continua senza",
  },
  es: {
    title: "Configurar Questkeep",
    step: "Paso {n} de 3",
    welcomeTitle: "Te damos la bienvenida a Questkeep",
    welcomeText:
      "Todos tus juegos de PC de todas las tiendas en una sola biblioteca. Questkeep ya busca las tiendas instaladas; los dos pasos siguientes te permiten iniciar sesión en las tiendas y configurar claves opcionales para obtener más detalles de los juegos. Todo esto se puede cambiar después en la configuración.",
    storesTitle: "Tus tiendas",
    keysTitle: "Más detalles de los juegos (opcional)",
    keysIntro:
      "Questkeep obtiene la mayoría de los detalles de tus tiendas. Dos servicios gratuitos aportan más; ambos necesitan una clave de tu propia cuenta gratuita, que se guarda solo en el almacén de credenciales de tu sistema.",
    igdbTitle: "IGDB (a través de Twitch)",
    recommended: "Recomendado",
    igdbDesc:
      "Tiempo para completar, enlaces a wiki y redes sociales, sagas, clasificaciones por edad, modos de juego, temáticas y detalles de los juegos que tus tiendas conocen poco.",
    optional: "Opcional",
    griddbDesc:
      "Portadas para los juegos que no tienen, y más portadas, fondos e iconos en el selector de imágenes.",
    warnTitle: "¿Continuar sin IGDB?",
    warnStrong: "Sin clave de IGDB, esto se queda vacío:",
    without1:
      "Tiempo para completar (solo quedan los datos de HowLongToBeat, si encuentra el juego)",
    without2:
      "Enlaces al sitio web, la wiki y las redes sociales (los enlaces de las tiendas siguen funcionando)",
    without3:
      "Sagas, clasificaciones por edad, modos de juego, temáticas, perspectivas y sus filtros",
    without4:
      "Detalles de los juegos de tiendas sin metadatos (Ubisoft, EA, Battle.net, añadidos a mano)",
    warnHint:
      "Puedes añadir la clave cuando quieras en Configuración → Metadatos y luego usar «Actualizar los metadatos de todos los juegos».",
    skipSetup: "Omitir la configuración",
    getStarted: "Empezar",
    back: "Atrás",
    next: "Siguiente",
    skip: "Omitir",
    finish: "Finalizar",
    saving: "Guardando…",
    backToKeys: "Volver a las claves",
    continueWithout: "Continuar sin",
  },
  pt: {
    title: "Configurar o Questkeep",
    step: "Etapa {n} de 3",
    welcomeTitle: "Bem-vindo ao Questkeep",
    welcomeText:
      "Todos os seus jogos de PC de todas as lojas em uma só biblioteca. O Questkeep já procura as lojas instaladas; as próximas duas etapas permitem entrar nas lojas e configurar chaves opcionais para detalhes mais completos dos jogos. Tudo isso pode ser alterado depois em Configurações.",
    storesTitle: "Suas lojas",
    keysTitle: "Detalhes mais completos dos jogos (opcional)",
    keysIntro:
      "O Questkeep obtém a maioria dos detalhes das suas lojas. Dois serviços gratuitos acrescentam mais; ambos precisam de uma chave da sua própria conta gratuita, guardada somente no cofre de credenciais do seu sistema.",
    igdbTitle: "IGDB (via Twitch)",
    recommended: "Recomendado",
    igdbDesc:
      "Tempo para zerar, links de wiki e redes sociais, séries, classificações indicativas, modos de jogo, temas e detalhes de jogos que suas lojas conhecem pouco.",
    optional: "Opcional",
    griddbDesc:
      "Capas para jogos que não têm, e mais capas, planos de fundo e ícones no seletor de imagens.",
    warnTitle: "Continuar sem o IGDB?",
    warnStrong: "Sem uma chave do IGDB, isto fica vazio:",
    without1:
      "Tempo para zerar (só restam os números do HowLongToBeat, quando ele encontra o jogo)",
    without2: "Links de site, wiki e redes sociais (os links das lojas continuam funcionando)",
    without3:
      "Séries, classificações indicativas, modos de jogo, temas, perspectivas e os filtros correspondentes",
    without4:
      "Detalhes de jogos de lojas sem metadados (Ubisoft, EA, Battle.net, adicionados manualmente)",
    warnHint:
      "Você pode adicionar a chave a qualquer momento em Configurações → Metadados e depois usar “Atualizar os metadados de todos os jogos”.",
    skipSetup: "Pular a configuração",
    getStarted: "Começar",
    back: "Voltar",
    next: "Avançar",
    skip: "Pular",
    finish: "Concluir",
    saving: "Salvando…",
    backToKeys: "Voltar às chaves",
    continueWithout: "Continuar sem",
  },
};
