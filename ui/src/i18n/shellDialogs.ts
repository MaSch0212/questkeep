import type { Translations } from ".";

const en = {
  // UpdateDialog
  updateTitle: "Update available",
  updateMessage: (p: { version: string; current: string | null }) =>
    `Questkeep ${p.version} is available${p.current ? ` (you have ${p.current})` : ""}. Installing it restarts Questkeep.`,
  later: "Later",
  installing: "Installing…",
  installAndRestart: "Install and restart",
  downloadLabel: "Update download",
  startingDownload: "Starting the download…",
  downloadProgress: "Downloading: {done} of {total}",
  downloadedSoFar: "Downloading: {done}",
  installingUpdate: "Installing… Questkeep restarts in a moment.",
  manualUpdateHint:
    "This copy was downloaded rather than installed, so it can't update itself. Download the new version from the release page.",
  openReleasePage: "Open download page",
  // QuitDuringBackupDialog
  backupTitle: "A backup is running",
  backupWriting:
    "Questkeep is writing a backup. Quitting now cancels it and deletes the unfinished archive.",
  finishingBackup: "Finishing the backup…",
  quitsWhenDone: "{label} Questkeep quits when it is done.",
  keepOpen: "Keep Questkeep open",
  quitNow: "Quit now",
  waitAndQuit: "Wait and quit",
};

/** The update dialog and the quit-during-backup dialog (UpdateDialog, QuitDuringBackupDialog). */
export const shellDialogs: Translations<typeof en> = {
  en,
  de: {
    updateTitle: "Update verfügbar",
    updateMessage: (p: { version: string; current: string | null }) =>
      `Questkeep ${p.version} ist verfügbar${p.current ? ` (du hast ${p.current})` : ""}. Die Installation startet Questkeep neu.`,
    later: "Später",
    installing: "Wird installiert…",
    installAndRestart: "Installieren und neu starten",
    downloadLabel: "Download des Updates",
    startingDownload: "Download wird gestartet…",
    downloadProgress: "Wird heruntergeladen: {done} von {total}",
    downloadedSoFar: "Wird heruntergeladen: {done}",
    installingUpdate: "Wird installiert… Questkeep startet gleich neu.",
    manualUpdateHint:
      "Diese Kopie wurde heruntergeladen statt installiert und kann sich daher nicht selbst aktualisieren. Lade die neue Version von der Release-Seite herunter.",
    openReleasePage: "Download-Seite öffnen",
    backupTitle: "Eine Sicherung läuft",
    backupWriting:
      "Questkeep schreibt gerade eine Sicherung. Beendest du jetzt, wird sie abgebrochen und das unfertige Archiv gelöscht.",
    finishingBackup: "Die Sicherung wird abgeschlossen…",
    quitsWhenDone: "{label} Questkeep wird beendet, sobald sie fertig ist.",
    keepOpen: "Questkeep geöffnet lassen",
    quitNow: "Jetzt beenden",
    waitAndQuit: "Warten und beenden",
  },
  fr: {
    updateTitle: "Mise à jour disponible",
    updateMessage: (p: { version: string; current: string | null }) =>
      `Questkeep ${p.version} est disponible${p.current ? ` (vous avez ${p.current})` : ""}. L'installation redémarre Questkeep.`,
    later: "Plus tard",
    installing: "Installation…",
    installAndRestart: "Installer et redémarrer",
    downloadLabel: "Téléchargement de la mise à jour",
    startingDownload: "Démarrage du téléchargement…",
    downloadProgress: "Téléchargement : {done} sur {total}",
    downloadedSoFar: "Téléchargement : {done}",
    installingUpdate: "Installation… Questkeep redémarre dans un instant.",
    manualUpdateHint:
      "Cette copie a été téléchargée et non installée : elle ne peut pas se mettre à jour seule. Téléchargez la nouvelle version depuis la page de la version.",
    openReleasePage: "Ouvrir la page de téléchargement",
    backupTitle: "Une sauvegarde est en cours",
    backupWriting:
      "Questkeep est en train d'écrire une sauvegarde. Quitter maintenant l'annule et supprime l'archive inachevée.",
    finishingBackup: "Finalisation de la sauvegarde…",
    quitsWhenDone: "{label} Questkeep se fermera une fois terminée.",
    keepOpen: "Garder Questkeep ouvert",
    quitNow: "Quitter maintenant",
    waitAndQuit: "Attendre et quitter",
  },
  it: {
    updateTitle: "Aggiornamento disponibile",
    updateMessage: (p: { version: string; current: string | null }) =>
      `Questkeep ${p.version} è disponibile${p.current ? ` (hai ${p.current})` : ""}. L'installazione riavvia Questkeep.`,
    later: "Più tardi",
    installing: "Installazione…",
    installAndRestart: "Installa e riavvia",
    downloadLabel: "Download dell'aggiornamento",
    startingDownload: "Avvio del download…",
    downloadProgress: "Download: {done} di {total}",
    downloadedSoFar: "Download: {done}",
    installingUpdate: "Installazione… Questkeep si riavvia tra un attimo.",
    manualUpdateHint:
      "Questa copia è stata scaricata, non installata, quindi non può aggiornarsi da sola. Scarica la nuova versione dalla pagina della release.",
    openReleasePage: "Apri la pagina di download",
    backupTitle: "Un backup è in corso",
    backupWriting:
      "Questkeep sta scrivendo un backup. Uscire ora lo annulla ed elimina l'archivio non completato.",
    finishingBackup: "Completamento del backup…",
    quitsWhenDone: "{label} Questkeep si chiude al termine.",
    keepOpen: "Mantieni Questkeep aperto",
    quitNow: "Esci ora",
    waitAndQuit: "Attendi ed esci",
  },
  es: {
    updateTitle: "Actualización disponible",
    updateMessage: (p: { version: string; current: string | null }) =>
      `Questkeep ${p.version} está disponible${p.current ? ` (tienes ${p.current})` : ""}. Al instalarla, Questkeep se reinicia.`,
    later: "Más tarde",
    installing: "Instalando…",
    installAndRestart: "Instalar y reiniciar",
    downloadLabel: "Descarga de la actualización",
    startingDownload: "Iniciando la descarga…",
    downloadProgress: "Descargando: {done} de {total}",
    downloadedSoFar: "Descargando: {done}",
    installingUpdate: "Instalando… Questkeep se reiniciará en un momento.",
    manualUpdateHint:
      "Esta copia se descargó en lugar de instalarse, así que no puede actualizarse sola. Descarga la nueva versión desde la página de la versión.",
    openReleasePage: "Abrir la página de descarga",
    backupTitle: "Hay una copia de seguridad en curso",
    backupWriting:
      "Questkeep está escribiendo una copia de seguridad. Si sales ahora, se cancela y se elimina el archivo sin terminar.",
    finishingBackup: "Terminando la copia de seguridad…",
    quitsWhenDone: "{label} Questkeep se cerrará cuando termine.",
    keepOpen: "Mantener Questkeep abierto",
    quitNow: "Salir ahora",
    waitAndQuit: "Esperar y salir",
  },
  pt: {
    updateTitle: "Atualização disponível",
    updateMessage: (p: { version: string; current: string | null }) =>
      `O Questkeep ${p.version} está disponível${p.current ? ` (você tem ${p.current})` : ""}. A instalação reinicia o Questkeep.`,
    later: "Mais tarde",
    installing: "Instalando…",
    installAndRestart: "Instalar e reiniciar",
    downloadLabel: "Download da atualização",
    startingDownload: "Iniciando o download…",
    downloadProgress: "Baixando: {done} de {total}",
    downloadedSoFar: "Baixando: {done}",
    installingUpdate: "Instalando… O Questkeep reinicia em instantes.",
    manualUpdateHint:
      "Esta cópia foi baixada em vez de instalada, por isso não pode se atualizar sozinha. Baixe a nova versão na página da versão.",
    openReleasePage: "Abrir a página de download",
    backupTitle: "Um backup está em andamento",
    backupWriting:
      "O Questkeep está gravando um backup. Sair agora o cancela e exclui o arquivo inacabado.",
    finishingBackup: "Concluindo o backup…",
    quitsWhenDone: "{label} O Questkeep fecha quando terminar.",
    keepOpen: "Manter o Questkeep aberto",
    quitNow: "Sair agora",
    waitAndQuit: "Aguardar e sair",
  },
};
