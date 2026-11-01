import type { Translations } from ".";

const games = (n: number) => `${n} ${n === 1 ? "game" : "games"}`;
const spiele = (n: number) => `${n} ${n === 1 ? "Spiel" : "Spiele"}`;
const jeux = (n: number) => `${n} ${n === 1 ? "jeu" : "jeux"}`;
const giochi = (n: number) => `${n} ${n === 1 ? "gioco" : "giochi"}`;
const juegos = (n: number) => `${n} ${n === 1 ? "juego" : "juegos"}`;
const jogos = (n: number) => `${n} ${n === 1 ? "jogo" : "jogos"}`;

const en = {
  // Modal, toasts, confirm dialog
  closeDialog: "Close dialog",
  dismissNotification: "Dismiss notification",
  cancel: "Cancel",
  // errors
  wentWrong: "Something went wrong.",
  // layout
  detailsPanelWidth: "Details panel width",
  // actions, bulk actions, game keys
  installHanded: "Install handed to the store client",
  installAllHanded: (p: { count: number }) =>
    `Install of ${games(p.count)} handed to the store clients`,
  refreshFailed: (p: { count: number }) => `Could not refresh ${games(p.count)}`,
  refreshing: (p: { count: number }) => `Refreshing metadata for ${games(p.count)}…`,
  removedGames: (p: { count: number }) => `Removed ${games(p.count)}`,
  unhid: "Unhid “{name}”",
  hid: "Hid “{name}”",
  // store
  duplicatesFound: (p: { count: number }) =>
    p.count === 1
      ? "1 game looks like it's in more than one store"
      : `${p.count} games look like they're in more than one store`,
  review: "Review",
  backupFailed: "Backup failed: {error}",
  // file dialogs
  chooseExecutable: "Choose the game executable",
  filterExecutable: "Executable",
  filterAllFiles: "All files",
  chooseCover: "Choose a cover image",
  filterImage: "Image",
  chooseFolder: "Choose a folder",
  chooseIcon: "Choose an icon",
  chooseImage: "Choose an image",
  filterIconOrImage: "Icon or image",
};

/** Shared texts of the shell: dialogs, toasts, errors, bulk actions, file dialogs. */
export const shellCommon: Translations<typeof en> = {
  en,
  de: {
    closeDialog: "Dialog schließen",
    dismissNotification: "Benachrichtigung schließen",
    cancel: "Abbrechen",
    wentWrong: "Etwas ist schiefgelaufen.",
    detailsPanelWidth: "Breite des Detailbereichs",
    installHanded: "Installation an den Store-Client übergeben",
    installAllHanded: (p: { count: number }) =>
      `Installation von ${spiele(p.count)} an die Store-Clients übergeben`,
    refreshFailed: (p: { count: number }) => `${spiele(p.count)} konnten nicht aktualisiert werden`,
    refreshing: (p: { count: number }) => `Metadaten für ${spiele(p.count)} werden aktualisiert…`,
    removedGames: (p: { count: number }) => `${spiele(p.count)} entfernt`,
    unhid: "„{name}“ eingeblendet",
    hid: "„{name}“ ausgeblendet",
    duplicatesFound: (p: { count: number }) =>
      p.count === 1
        ? "1 Spiel scheint in mehr als einem Store zu sein"
        : `${p.count} Spiele scheinen in mehr als einem Store zu sein`,
    review: "Prüfen",
    backupFailed: "Sicherung fehlgeschlagen: {error}",
    chooseExecutable: "Ausführbare Datei des Spiels wählen",
    filterExecutable: "Ausführbare Datei",
    filterAllFiles: "Alle Dateien",
    chooseCover: "Cover-Bild wählen",
    filterImage: "Bild",
    chooseFolder: "Ordner wählen",
    chooseIcon: "Symbol wählen",
    chooseImage: "Bild wählen",
    filterIconOrImage: "Symbol oder Bild",
  },
  fr: {
    closeDialog: "Fermer la boîte de dialogue",
    dismissNotification: "Fermer la notification",
    cancel: "Annuler",
    wentWrong: "Une erreur s'est produite.",
    detailsPanelWidth: "Largeur du panneau de détails",
    installHanded: "Installation transmise au client de la boutique",
    installAllHanded: (p: { count: number }) =>
      `Installation de ${jeux(p.count)} transmise aux clients des boutiques`,
    refreshFailed: (p: { count: number }) => `Impossible d'actualiser ${jeux(p.count)}`,
    refreshing: (p: { count: number }) => `Actualisation des métadonnées de ${jeux(p.count)}…`,
    removedGames: (p: { count: number }) =>
      p.count === 1 ? "1 jeu retiré" : `${p.count} jeux retirés`,
    unhid: "« {name} » n'est plus masqué",
    hid: "« {name} » masqué",
    duplicatesFound: (p: { count: number }) =>
      p.count === 1
        ? "1 jeu semble être dans plusieurs boutiques"
        : `${p.count} jeux semblent être dans plusieurs boutiques`,
    review: "Examiner",
    backupFailed: "Échec de la sauvegarde : {error}",
    chooseExecutable: "Choisir l'exécutable du jeu",
    filterExecutable: "Exécutable",
    filterAllFiles: "Tous les fichiers",
    chooseCover: "Choisir une image de jaquette",
    filterImage: "Image",
    chooseFolder: "Choisir un dossier",
    chooseIcon: "Choisir une icône",
    chooseImage: "Choisir une image",
    filterIconOrImage: "Icône ou image",
  },
  it: {
    closeDialog: "Chiudi la finestra di dialogo",
    dismissNotification: "Chiudi la notifica",
    cancel: "Annulla",
    wentWrong: "Qualcosa è andato storto.",
    detailsPanelWidth: "Larghezza del pannello dei dettagli",
    installHanded: "Installazione passata al client dello store",
    installAllHanded: (p: { count: number }) =>
      `Installazione di ${giochi(p.count)} passata ai client degli store`,
    refreshFailed: (p: { count: number }) => `Impossibile aggiornare ${giochi(p.count)}`,
    refreshing: (p: { count: number }) => `Aggiornamento dei metadati di ${giochi(p.count)}…`,
    removedGames: (p: { count: number }) =>
      p.count === 1 ? "1 gioco rimosso" : `${p.count} giochi rimossi`,
    unhid: "“{name}” non è più nascosto",
    hid: "“{name}” nascosto",
    duplicatesFound: (p: { count: number }) =>
      p.count === 1
        ? "1 gioco sembra essere in più di uno store"
        : `${p.count} giochi sembrano essere in più di uno store`,
    review: "Controlla",
    backupFailed: "Backup non riuscito: {error}",
    chooseExecutable: "Scegli l'eseguibile del gioco",
    filterExecutable: "Eseguibile",
    filterAllFiles: "Tutti i file",
    chooseCover: "Scegli un'immagine di copertina",
    filterImage: "Immagine",
    chooseFolder: "Scegli una cartella",
    chooseIcon: "Scegli un'icona",
    chooseImage: "Scegli un'immagine",
    filterIconOrImage: "Icona o immagine",
  },
  es: {
    closeDialog: "Cerrar el cuadro de diálogo",
    dismissNotification: "Descartar la notificación",
    cancel: "Cancelar",
    wentWrong: "Algo salió mal.",
    detailsPanelWidth: "Ancho del panel de detalles",
    installHanded: "Instalación enviada al cliente de la tienda",
    installAllHanded: (p: { count: number }) =>
      `Instalación de ${juegos(p.count)} enviada a los clientes de las tiendas`,
    refreshFailed: (p: { count: number }) => `No se pudo actualizar ${juegos(p.count)}`,
    refreshing: (p: { count: number }) => `Actualizando los metadatos de ${juegos(p.count)}…`,
    removedGames: (p: { count: number }) =>
      p.count === 1 ? "1 juego quitado" : `${p.count} juegos quitados`,
    unhid: "«{name}» ya no está oculto",
    hid: "«{name}» oculto",
    duplicatesFound: (p: { count: number }) =>
      p.count === 1
        ? "1 juego parece estar en más de una tienda"
        : `${p.count} juegos parecen estar en más de una tienda`,
    review: "Revisar",
    backupFailed: "Error en la copia de seguridad: {error}",
    chooseExecutable: "Elige el ejecutable del juego",
    filterExecutable: "Ejecutable",
    filterAllFiles: "Todos los archivos",
    chooseCover: "Elige una imagen de portada",
    filterImage: "Imagen",
    chooseFolder: "Elige una carpeta",
    chooseIcon: "Elige un icono",
    chooseImage: "Elige una imagen",
    filterIconOrImage: "Icono o imagen",
  },
  pt: {
    closeDialog: "Fechar a caixa de diálogo",
    dismissNotification: "Dispensar a notificação",
    cancel: "Cancelar",
    wentWrong: "Algo deu errado.",
    detailsPanelWidth: "Largura do painel de detalhes",
    installHanded: "Instalação enviada ao cliente da loja",
    installAllHanded: (p: { count: number }) =>
      `Instalação de ${jogos(p.count)} enviada aos clientes das lojas`,
    refreshFailed: (p: { count: number }) => `Não foi possível atualizar ${jogos(p.count)}`,
    refreshing: (p: { count: number }) => `Atualizando os metadados de ${jogos(p.count)}…`,
    removedGames: (p: { count: number }) =>
      p.count === 1 ? "1 jogo removido" : `${p.count} jogos removidos`,
    unhid: "“{name}” não está mais oculto",
    hid: "“{name}” oculto",
    duplicatesFound: (p: { count: number }) =>
      p.count === 1
        ? "1 jogo parece estar em mais de uma loja"
        : `${p.count} jogos parecem estar em mais de uma loja`,
    review: "Revisar",
    backupFailed: "Falha no backup: {error}",
    chooseExecutable: "Escolha o executável do jogo",
    filterExecutable: "Executável",
    filterAllFiles: "Todos os arquivos",
    chooseCover: "Escolha uma imagem de capa",
    filterImage: "Imagem",
    chooseFolder: "Escolha uma pasta",
    chooseIcon: "Escolha um ícone",
    chooseImage: "Escolha uma imagem",
    filterIconOrImage: "Ícone ou imagem",
  },
};
