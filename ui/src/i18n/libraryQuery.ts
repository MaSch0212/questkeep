import type { Translations } from ".";

const en = {
  scanSummary: "{added} added, {updated} updated, {uninstalled} uninstalled",
};

/** The result line of a store scan (lib/query.ts). */
export const libraryQuery: Translations<typeof en> = {
  en,
  de: { scanSummary: "{added} hinzugefügt, {updated} aktualisiert, {uninstalled} deinstalliert" },
  fr: { scanSummary: "{added} ajoutés, {updated} mis à jour, {uninstalled} désinstallés" },
  it: { scanSummary: "{added} aggiunti, {updated} aggiornati, {uninstalled} disinstallati" },
  es: { scanSummary: "{added} añadidos, {updated} actualizados, {uninstalled} desinstalados" },
  pt: { scanSummary: "{added} adicionados, {updated} atualizados, {uninstalled} desinstalados" },
};
