import type { Translations } from ".";

const en = {
  add: "Add…",
  remove: "Remove {value}",
};

/** The list-of-chips input (ChipInput). */
export const settingsChipInput: Translations<typeof en> = {
  en,
  de: { add: "Hinzufügen…", remove: "{value} entfernen" },
  fr: { add: "Ajouter…", remove: "Supprimer {value}" },
  it: { add: "Aggiungi…", remove: "Rimuovi {value}" },
  es: { add: "Añadir…", remove: "Quitar {value}" },
  pt: { add: "Adicionar…", remove: "Remover {value}" },
};
