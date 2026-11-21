import type { Translations } from ".";

const en = {
  title: "Plugin updates",
  intro:
    "These plugins from the registry have a newer version. Nothing is installed without your go-ahead.",
  list: "Available updates",
  none: "Everything is up to date.",
  update: "Update",
  updateLabel: "Update {name} to {version}",
  updateAll: "Update all",
  updateAllHint:
    "Updates every plugin that asks for nothing new. The others stay for you to review.",
  downloading: "Downloading {done} of {total}…",
  installing: "Installing {done} of {total}…",
  notes: "What changed",
  reviewNeeded: "Needs your review: asks for {what}.",
  reviewHintList: "Update all skips it: use its Update button.",
  askHttp: "web access to {values}",
  askRead: "read access to {values}",
  askFolders: "a folder you choose: {values}",
  askExtension: "an extension (it runs inside the app)",
  askStore: "another store",
  askFilesystem: "read access to every file on this PC",
  askRegistry: "read access to the Windows registry",
  askLauncher: "closing these programs after a game: {values}",
  askLogin: "sign-in windows on {values}",
  confirmTitle: (p: { count: number }) =>
    p.count === 1 ? "Update 1 plugin" : `Update ${p.count} plugins`,
  confirmIntro:
    "These updates ask for nothing new: they keep the permissions and folders you already agreed to.",
  confirmExtension:
    "{names} runs an extension: JavaScript that runs inside the app with the app's own rights. Only update plugins you trust.",
  back: "Back",
  close: "Close",
  results: "Results",
  resultUpdated: "Updated to {version}",
  resultFailed: "Failed: {message}",
  resultReview: "Not updated: needs your review (asks for {what}).",
  reviewHintResult: "Use Update for this plugin instead of Update all.",
  summary: "Updated: {updated} · Failed: {failed} · To review: {review}",
  updatedToast: (p: { count: number }) =>
    p.count === 1 ? "Updated 1 plugin." : `Updated ${p.count} plugins.`,
  updatesButton: "Updates",
  updatesCount: "Plugin updates available: {count}",
  checkSetting: "Check for plugin updates",
  checkHint:
    "Looks for new versions of plugins from the registry a little after start and every 12 hours. Updates are never installed on their own.",
  checkNow: "Check now",
  checking: "Checking…",
  upToDate: "Everything is up to date.",
  found: (p: { count: number }) =>
    p.count === 1 ? "1 plugin update available." : `${p.count} plugin updates available.`,
  nothingToCheck: "No installed plugin comes from the registry, so there is nothing to check.",
};

/** The Plugin updates dialog and the update check in Settings → Plugins. */
export const pluginUpdates: Translations<typeof en> = {
  en,
  de: {
    title: "Plugin-Updates",
    intro:
      "Für diese Plugins aus der Registry gibt es eine neuere Version. Ohne dein Okay wird nichts installiert.",
    list: "Verfügbare Updates",
    none: "Alles ist auf dem neuesten Stand.",
    update: "Aktualisieren",
    updateLabel: "{name} auf {version} aktualisieren",
    updateAll: "Alle aktualisieren",
    updateAllHint:
      "Aktualisiert jedes Plugin, das nichts Neues verlangt. Die anderen bleiben zur Prüfung in der Liste.",
    downloading: "Wird heruntergeladen: {done} von {total}…",
    installing: "Wird installiert: {done} von {total}…",
    notes: "Änderungen",
    reviewNeeded: "Bitte prüfen: verlangt {what}.",
    reviewHintList:
      "„Alle aktualisieren“ überspringt es: nutze die Schaltfläche „Aktualisieren“ des Plugins.",
    askHttp: "Webzugriff auf {values}",
    askRead: "Lesezugriff auf {values}",
    askFolders: "einen Ordner, den du wählst: {values}",
    askExtension: "eine Erweiterung (läuft in der App)",
    askStore: "einen weiteren Store",
    askFilesystem: "Lesezugriff auf jede Datei dieses PCs",
    askRegistry: "Lesezugriff auf die Windows-Registry",
    askLauncher: "das Schließen dieser Programme nach einem Spiel: {values}",
    askLogin: "Anmeldefenster auf {values}",
    confirmTitle: (p) =>
      p.count === 1 ? "1 Plugin aktualisieren" : `${p.count} Plugins aktualisieren`,
    confirmIntro:
      "Diese Updates verlangen nichts Neues: Sie behalten die Berechtigungen und Ordner, denen du schon zugestimmt hast.",
    confirmExtension:
      "{names} führt eine Erweiterung aus: JavaScript, das mit den Rechten der App in der App läuft. Aktualisiere nur Plugins, denen du vertraust.",
    back: "Zurück",
    close: "Schließen",
    results: "Ergebnisse",
    resultUpdated: "Auf {version} aktualisiert",
    resultFailed: "Fehlgeschlagen: {message}",
    resultReview: "Nicht aktualisiert: bitte prüfen (verlangt {what}).",
    reviewHintResult: "Nutze „Aktualisieren“ bei diesem Plugin statt „Alle aktualisieren“.",
    summary: "Aktualisiert: {updated} · Fehlgeschlagen: {failed} · Zu prüfen: {review}",
    updatedToast: (p) =>
      p.count === 1 ? "1 Plugin aktualisiert." : `${p.count} Plugins aktualisiert.`,
    updatesButton: "Updates",
    updatesCount: "Verfügbare Plugin-Updates: {count}",
    checkSetting: "Nach Plugin-Updates suchen",
    checkHint:
      "Sucht kurz nach dem Start und danach alle 12 Stunden nach neuen Versionen der Plugins aus der Registry. Updates werden nie von selbst installiert.",
    checkNow: "Jetzt prüfen",
    checking: "Wird geprüft…",
    upToDate: "Alles ist auf dem neuesten Stand.",
    found: (p) =>
      p.count === 1 ? "1 Plugin-Update verfügbar." : `${p.count} Plugin-Updates verfügbar.`,
    nothingToCheck:
      "Kein installiertes Plugin stammt aus der Registry, daher gibt es nichts zu prüfen.",
  },
  fr: {
    title: "Mises à jour des plugins",
    intro:
      "Ces plugins du registre ont une version plus récente. Rien n’est installé sans votre accord.",
    list: "Mises à jour disponibles",
    none: "Tout est à jour.",
    update: "Mettre à jour",
    updateLabel: "Mettre à jour {name} vers {version}",
    updateAll: "Tout mettre à jour",
    updateAllHint:
      "Met à jour chaque plugin qui ne demande rien de nouveau. Les autres restent à examiner.",
    downloading: "Téléchargement : {done} sur {total}…",
    installing: "Installation : {done} sur {total}…",
    notes: "Nouveautés",
    reviewNeeded: "À vérifier : demande {what}.",
    reviewHintList: "« Tout mettre à jour » l'ignore : utilise son bouton Mettre à jour.",
    askHttp: "un accès web à {values}",
    askRead: "un accès en lecture à {values}",
    askFolders: "un dossier que vous choisissez : {values}",
    askExtension: "une extension (elle s’exécute dans l’application)",
    askStore: "une autre boutique",
    askFilesystem: "un accès en lecture à tous les fichiers de ce PC",
    askRegistry: "un accès en lecture au registre Windows",
    askLauncher: "la fermeture de ces programmes après une partie : {values}",
    askLogin: "des fenêtres de connexion sur {values}",
    confirmTitle: (p) =>
      p.count === 1 ? "Mettre à jour 1 plugin" : `Mettre à jour ${p.count} plugins`,
    confirmIntro:
      "Ces mises à jour ne demandent rien de nouveau : elles gardent les autorisations et les dossiers déjà acceptés.",
    confirmExtension:
      "{names} exécute une extension : du JavaScript qui tourne dans l’application avec ses propres droits. Ne mettez à jour que des plugins de confiance.",
    back: "Retour",
    close: "Fermer",
    results: "Résultats",
    resultUpdated: "Mis à jour vers {version}",
    resultFailed: "Échec : {message}",
    resultReview: "Non mis à jour : à vérifier (demande {what}).",
    reviewHintResult: "Utilise Mettre à jour pour ce plugin au lieu de Tout mettre à jour.",
    summary: "Mis à jour : {updated} · Échecs : {failed} · À vérifier : {review}",
    updatedToast: (p) =>
      p.count === 1 ? "1 plugin mis à jour." : `${p.count} plugins mis à jour.`,
    updatesButton: "Mises à jour",
    updatesCount: "Mises à jour de plugins disponibles : {count}",
    checkSetting: "Rechercher les mises à jour des plugins",
    checkHint:
      "Cherche de nouvelles versions des plugins du registre peu après le démarrage, puis toutes les 12 heures. Les mises à jour ne s’installent jamais toutes seules.",
    checkNow: "Vérifier maintenant",
    checking: "Vérification…",
    upToDate: "Tout est à jour.",
    found: (p) =>
      p.count === 1
        ? "1 mise à jour de plugin disponible."
        : `${p.count} mises à jour de plugins disponibles.`,
    nothingToCheck: "Aucun plugin installé ne vient du registre : il n’y a rien à vérifier.",
  },
  it: {
    title: "Aggiornamenti dei plugin",
    intro:
      "Questi plugin del registro hanno una versione più recente. Non viene installato nulla senza il tuo consenso.",
    list: "Aggiornamenti disponibili",
    none: "Tutto è aggiornato.",
    update: "Aggiorna",
    updateLabel: "Aggiorna {name} alla versione {version}",
    updateAll: "Aggiorna tutto",
    updateAllHint:
      "Aggiorna ogni plugin che non chiede nulla di nuovo. Gli altri restano da controllare.",
    downloading: "Download: {done} di {total}…",
    installing: "Installazione: {done} di {total}…",
    notes: "Novità",
    reviewNeeded: "Da controllare: richiede {what}.",
    reviewHintList: "« Aggiorna tutto » lo salta: usa il suo pulsante Aggiorna.",
    askHttp: "accesso al web verso {values}",
    askRead: "accesso in lettura a {values}",
    askFolders: "una cartella che scegli tu: {values}",
    askExtension: "un’estensione (gira nell’app)",
    askStore: "un altro store",
    askFilesystem: "accesso in lettura a tutti i file di questo PC",
    askRegistry: "accesso in lettura al registro di Windows",
    askLauncher: "la chiusura di questi programmi dopo una partita: {values}",
    askLogin: "finestre di accesso su {values}",
    confirmTitle: (p) => (p.count === 1 ? "Aggiorna 1 plugin" : `Aggiorna ${p.count} plugin`),
    confirmIntro:
      "Questi aggiornamenti non chiedono nulla di nuovo: mantengono i permessi e le cartelle già accettati.",
    confirmExtension:
      "{names} esegue un’estensione: JavaScript che gira nell’app con i diritti dell’app stessa. Aggiorna solo plugin di cui ti fidi.",
    back: "Indietro",
    close: "Chiudi",
    results: "Risultati",
    resultUpdated: "Aggiornato alla versione {version}",
    resultFailed: "Non riuscito: {message}",
    resultReview: "Non aggiornato: da controllare (richiede {what}).",
    reviewHintResult: "Usa Aggiorna per questo plugin invece di Aggiorna tutto.",
    summary: "Aggiornati: {updated} · Falliti: {failed} · Da controllare: {review}",
    updatedToast: (p) => (p.count === 1 ? "1 plugin aggiornato." : `${p.count} plugin aggiornati.`),
    updatesButton: "Aggiornamenti",
    updatesCount: "Aggiornamenti dei plugin disponibili: {count}",
    checkSetting: "Cerca aggiornamenti dei plugin",
    checkHint:
      "Cerca nuove versioni dei plugin del registro poco dopo l’avvio e poi ogni 12 ore. Gli aggiornamenti non si installano mai da soli.",
    checkNow: "Controlla ora",
    checking: "Controllo in corso…",
    upToDate: "Tutto è aggiornato.",
    found: (p) =>
      p.count === 1
        ? "1 aggiornamento di plugin disponibile."
        : `${p.count} aggiornamenti di plugin disponibili.`,
    nothingToCheck:
      "Nessun plugin installato proviene dal registro, quindi non c’è nulla da controllare.",
  },
  es: {
    title: "Actualizaciones de plugins",
    intro:
      "Estos plugins del registro tienen una versión más reciente. No se instala nada sin tu permiso.",
    list: "Actualizaciones disponibles",
    none: "Todo está al día.",
    update: "Actualizar",
    updateLabel: "Actualizar {name} a la versión {version}",
    updateAll: "Actualizar todo",
    updateAllHint:
      "Actualiza cada plugin que no pide nada nuevo. Los demás quedan para que los revises.",
    downloading: "Descargando {done} de {total}…",
    installing: "Instalando {done} de {total}…",
    notes: "Novedades",
    reviewNeeded: "Requiere tu revisión: pide {what}.",
    reviewHintList: "«Actualizar todo» lo omite: usa su botón Actualizar.",
    askHttp: "acceso web a {values}",
    askRead: "acceso de lectura a {values}",
    askFolders: "una carpeta que tú eliges: {values}",
    askExtension: "una extensión (se ejecuta dentro de la aplicación)",
    askStore: "otra tienda",
    askFilesystem: "acceso de lectura a todos los archivos de este equipo",
    askRegistry: "acceso de lectura al registro de Windows",
    askLauncher: "cerrar estos programas después de una partida: {values}",
    askLogin: "ventanas de inicio de sesión en {values}",
    confirmTitle: (p) => (p.count === 1 ? "Actualizar 1 plugin" : `Actualizar ${p.count} plugins`),
    confirmIntro:
      "Estas actualizaciones no piden nada nuevo: conservan los permisos y las carpetas que ya aceptaste.",
    confirmExtension:
      "{names} ejecuta una extensión: JavaScript que se ejecuta dentro de la aplicación con los derechos de la propia aplicación. Actualiza solo plugins de confianza.",
    back: "Atrás",
    close: "Cerrar",
    results: "Resultados",
    resultUpdated: "Actualizado a la versión {version}",
    resultFailed: "Error: {message}",
    resultReview: "No actualizado: requiere tu revisión (pide {what}).",
    reviewHintResult: "Usa Actualizar en este plugin en lugar de Actualizar todo.",
    summary: "Actualizados: {updated} · Con error: {failed} · Por revisar: {review}",
    updatedToast: (p) =>
      p.count === 1 ? "1 plugin actualizado." : `${p.count} plugins actualizados.`,
    updatesButton: "Actualizaciones",
    updatesCount: "Actualizaciones de plugins disponibles: {count}",
    checkSetting: "Buscar actualizaciones de plugins",
    checkHint:
      "Busca versiones nuevas de los plugins del registro poco después de iniciar y luego cada 12 horas. Las actualizaciones nunca se instalan solas.",
    checkNow: "Comprobar ahora",
    checking: "Comprobando…",
    upToDate: "Todo está al día.",
    found: (p) =>
      p.count === 1
        ? "1 actualización de plugin disponible."
        : `${p.count} actualizaciones de plugins disponibles.`,
    nothingToCheck:
      "Ningún plugin instalado procede del registro, así que no hay nada que comprobar.",
  },
  pt: {
    title: "Atualizações de plugins",
    intro:
      "Estes plugins do registo têm uma versão mais recente. Nada é instalado sem a sua autorização.",
    list: "Atualizações disponíveis",
    none: "Tudo está atualizado.",
    update: "Atualizar",
    updateLabel: "Atualizar {name} para a versão {version}",
    updateAll: "Atualizar tudo",
    updateAllHint:
      "Atualiza todos os plugins que não pedem nada de novo. Os restantes ficam para você rever.",
    downloading: "A transferir {done} de {total}…",
    installing: "A instalar {done} de {total}…",
    notes: "O que mudou",
    reviewNeeded: "Precisa da sua revisão: pede {what}.",
    reviewHintList: "«Atualizar tudo» ignora-o: use o botão Atualizar do plugin.",
    askHttp: "acesso à web a {values}",
    askRead: "acesso de leitura a {values}",
    askFolders: "uma pasta que você escolhe: {values}",
    askExtension: "uma extensão (corre dentro da aplicação)",
    askStore: "outra loja",
    askFilesystem: "acesso de leitura a todos os arquivos deste PC",
    askRegistry: "acesso de leitura ao registro do Windows",
    askLauncher: "fechar estes programas depois de um jogo: {values}",
    askLogin: "janelas de login em {values}",
    confirmTitle: (p) => (p.count === 1 ? "Atualizar 1 plugin" : `Atualizar ${p.count} plugins`),
    confirmIntro:
      "Estas atualizações não pedem nada de novo: mantêm as permissões e as pastas que já aceitou.",
    confirmExtension:
      "{names} executa uma extensão: JavaScript que corre dentro da aplicação com os direitos da própria aplicação. Atualize apenas plugins em que confia.",
    back: "Voltar",
    close: "Fechar",
    results: "Resultados",
    resultUpdated: "Atualizado para a versão {version}",
    resultFailed: "Falhou: {message}",
    resultReview: "Não atualizado: precisa da sua revisão (pede {what}).",
    reviewHintResult: "Use Atualizar neste plugin em vez de Atualizar tudo.",
    summary: "Atualizados: {updated} · Falhados: {failed} · Para rever: {review}",
    updatedToast: (p) =>
      p.count === 1 ? "1 plugin atualizado." : `${p.count} plugins atualizados.`,
    updatesButton: "Atualizações",
    updatesCount: "Atualizações de plugins disponíveis: {count}",
    checkSetting: "Procurar atualizações de plugins",
    checkHint:
      "Procura novas versões dos plugins do registo pouco depois do arranque e depois a cada 12 horas. As atualizações nunca são instaladas sozinhas.",
    checkNow: "Verificar agora",
    checking: "A verificar…",
    upToDate: "Tudo está atualizado.",
    found: (p) =>
      p.count === 1
        ? "1 atualização de plugin disponível."
        : `${p.count} atualizações de plugins disponíveis.`,
    nothingToCheck: "Nenhum plugin instalado vem do registo, por isso não há nada a verificar.",
  },
};
