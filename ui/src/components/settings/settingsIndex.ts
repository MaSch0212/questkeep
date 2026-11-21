import { translator } from "../../i18n";
import { pluginUpdates } from "../../i18n/pluginUpdates";
import { settingsAppearance } from "../../i18n/settingsAppearance";
import { settingsBackups } from "../../i18n/settingsBackups";
import { settingsGeneral } from "../../i18n/settingsGeneral";
import { settingsKeys } from "../../i18n/settingsKeys";
import type { SearchEntry } from "../../lib/settingsSearch";

const general = translator(settingsGeneral);
const appearance = translator(settingsAppearance);
const backups = translator(settingsBackups);
const keys = translator(settingsKeys);
const plugins = translator(pluginUpdates);

function e(id: string, page: string, label: string, part?: string, keywords?: string): SearchEntry {
  return { id, page, part, label, keywords };
}

/**
 * The single settings the search finds, besides the pages: where each lives (`page`, and `part`
 * on a page with several) and the text it has there, in the current language. Add a line here
 * when a setting is added.
 */
export function settingEntries(): SearchEntry[] {
  return [
    e("language", "general", general("language")),
    e("scanOnStartup", "general", general("scanOnStartup")),
    e("autoSync", "general", general("autoSync")),
    e("watchInstallFolders", "general", general("watchInstallFolders")),
    e("detectExternalLaunches", "general", general("detectExternalLaunches")),
    e("showStartPage", "general", general("showStartPage")),
    e("showStatistics", "general", general("showStatistics")),
    e("whenOpens", "general", general("whenOpens")),
    e("ignorePrefixes", "general", general("ignorePrefixes")),
    e("wordsToIgnore", "general", general("wordsToIgnore")),
    e("systemNotifications", "general", general("systemNotifications")),

    e("appearanceMode", "appearance", appearance("mode")),
    e("appearanceTheme", "appearance", appearance("theme")),

    e("startWithSystem", "windowTray", general("startWithSystem")),
    e("startMinimized", "windowTray", general("startMinimized")),
    e("closeToTray", "windowTray", general("closeToTray")),
    e("minimizeToTray", "windowTray", general("minimizeToTray")),
    e("trayIcon", "windowTray", general("trayIcon")),
    e("quickLaunchRows", "windowTray", general("quickLaunchRows")),
    e("quickLaunch", "windowTray", general("quickLaunch")),
    e("onGameStart", "windowTray", general("gameStartsGroup")),
    e("restoreAfterGame", "windowTray", general("restoreAfterGame")),
    e("closeDelay", "windowTray", general("closeDelay")),
    e("closeMinMinutes", "windowTray", general("closeMinMinutes")),

    e("downloadCovers", "artworkMetadata", general("downloadCovers"), "artwork"),
    e("griddbKey", "artworkMetadata", keys("griddbKey"), "artwork", "api"),
    e("fetchMetadata", "artworkMetadata", general("fetchMetadata"), "metadata"),
    e("igdbClientId", "artworkMetadata", keys("igdbClientId"), "metadata", "twitch api"),
    e("igdbClientSecret", "artworkMetadata", keys("igdbClientSecret"), "metadata", "twitch api"),

    e("checkPluginUpdates", "plugins", plugins("checkSetting")),

    e("backupAuto", "backups", backups("auto")),
    e("backupHowOften", "backups", backups("howOften")),
    e("backupKeep", "backups", backups("keep")),
    e("backupFolder", "backups", backups("folder")),
  ];
}
