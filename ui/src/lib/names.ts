const GENERIC_STEMS = new Set([
  "game",
  "launcher",
  "start",
  "play",
  "run",
  "main",
  "app",
  "client",
  "bin",
]);

function splitPath(path: string): string[] {
  return path.split(/[\\/]+/).filter((part) => part.length > 0);
}

function stem(fileName: string): string {
  const dot = fileName.lastIndexOf(".");
  return dot > 0 ? fileName.slice(0, dot) : fileName;
}

function tidy(value: string): string {
  return value.replaceAll("_", " ").replace(/\s+/g, " ").trim();
}

/**
 * Mirrors the backend rule: file stem, but the parent folder name when the stem is generic
 * (game, launcher, ...). Underscores become spaces.
 */
export function deriveGameName(path: string): string {
  const parts = splitPath(path);
  const file = parts.at(-1);
  if (!file) return "";
  const fileStem = stem(file);
  const parent = parts.at(-2);
  if (GENERIC_STEMS.has(fileStem.toLowerCase()) && parent !== undefined) return tidy(parent);
  return tidy(fileStem);
}
