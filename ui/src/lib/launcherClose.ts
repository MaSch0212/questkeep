import type { LauncherClose } from "../bindings/LauncherClose";
import type { LauncherClosePolicy } from "../bindings/LauncherClosePolicy";
import type { Settings } from "../bindings/Settings";

/** The choices of the "Close <store> after playing" setting, in display order. */
export const CLOSE_POLICIES: readonly LauncherClosePolicy[] = [
  "never",
  "always",
  "ifStartedByGame",
];

/** What the user chose for a store (a store without an entry: never). */
export function launcherClosePolicy(
  settings: Pick<Settings, "launcherClose"> | null,
  storeId: string,
): LauncherClosePolicy {
  return settings?.launcherClose[storeId] ?? "never";
}

/** The setting with one store's choice changed; "never" is the default and is not stored. */
export function withLauncherClose(
  launcherClose: Settings["launcherClose"],
  storeId: string,
  policy: LauncherClosePolicy,
): Settings["launcherClose"] {
  const { [storeId]: _old, ...rest } = launcherClose;
  return policy === "never" ? rest : { ...rest, [storeId]: policy };
}

/** How a launcher is ended, which decides the hint under the choice. */
export function closeKind(close: LauncherClose): "command" | "closeWindows" | "forceQuit" {
  if (close === "forceQuit") return "forceQuit";
  return "command" in close ? "command" : "closeWindows";
}
