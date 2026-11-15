import Archive from "lucide-solid/icons/archive";
import Cpu from "lucide-solid/icons/cpu";
import Download from "lucide-solid/icons/download";
import FileText from "lucide-solid/icons/file-text";
import FolderOpen from "lucide-solid/icons/folder-open";
import Globe from "lucide-solid/icons/globe";
import HardDrive from "lucide-solid/icons/hard-drive";
import Play from "lucide-solid/icons/play";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import Rocket from "lucide-solid/icons/rocket";
import Save from "lucide-solid/icons/save";
import Settings from "lucide-solid/icons/settings";
import Terminal from "lucide-solid/icons/terminal";
import Upload from "lucide-solid/icons/upload";
import Wrench from "lucide-solid/icons/wrench";
import Zap from "lucide-solid/icons/zap";
import type { JSX } from "solid-js";
import { Dynamic } from "solid-js/web";

type IconComponent = (props: JSX.SvgSVGAttributes<SVGSVGElement>) => JSX.Element;

/** The icons an action's button can have, by the (Lucide) name stored with the script. */
export const SCRIPT_ICONS: Readonly<Record<string, IconComponent>> = {
  zap: Zap,
  "folder-open": FolderOpen,
  save: Save,
  play: Play,
  terminal: Terminal,
  wrench: Wrench,
  download: Download,
  upload: Upload,
  "refresh-cw": RefreshCw,
  settings: Settings,
  archive: Archive,
  "file-text": FileText,
  globe: Globe,
  rocket: Rocket,
  cpu: Cpu,
  "hard-drive": HardDrive,
};

export const SCRIPT_ICON_NAMES = Object.keys(SCRIPT_ICONS);

/** The icon of an action (a bolt for one without or with a name that is not offered). */
export function ScriptIcon(props: { name: string | null }) {
  return (
    <Dynamic
      component={(props.name !== null ? SCRIPT_ICONS[props.name] : undefined) ?? Zap}
      class="icon"
      aria-hidden="true"
    />
  );
}

/** A readable name for an icon ("folder-open" → "folder open"), for tooltips and labels. */
export const iconLabel = (name: string): string => name.replaceAll("-", " ");
