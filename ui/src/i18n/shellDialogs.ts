import type { Translations } from ".";

const en = {
  // UpdateDialog
  updateTitle: "Update available",
  updateMessage: (p: { version: string; current: string | null }) =>
    `Questkeep ${p.version} is available${p.current ? ` (you have ${p.current})` : ""}. Installing it restarts Questkeep.`,
  later: "Later",
  installing: "Installing…",
  installAndRestart: "Install and restart",
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
