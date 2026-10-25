import Archive from "lucide-solid/icons/archive";
import Bookmark from "lucide-solid/icons/bookmark";
import Box from "lucide-solid/icons/box";
import Brain from "lucide-solid/icons/brain";
import Car from "lucide-solid/icons/car";
import Clock from "lucide-solid/icons/clock";
import Crown from "lucide-solid/icons/crown";
import Dices from "lucide-solid/icons/dices";
import EyeOff from "lucide-solid/icons/eye-off";
import Flame from "lucide-solid/icons/flame";
import Gamepad2 from "lucide-solid/icons/gamepad-2";
import Ghost from "lucide-solid/icons/ghost";
import HardDrive from "lucide-solid/icons/hard-drive";
import Heart from "lucide-solid/icons/heart";
import Joystick from "lucide-solid/icons/joystick";
import Library from "lucide-solid/icons/library";
import ListChecks from "lucide-solid/icons/list-checks";
import Map from "lucide-solid/icons/map";
import Mountain from "lucide-solid/icons/mountain";
import Puzzle from "lucide-solid/icons/puzzle";
import Rocket from "lucide-solid/icons/rocket";
import Shield from "lucide-solid/icons/shield";
import Skull from "lucide-solid/icons/skull";
import Sparkles from "lucide-solid/icons/sparkles";
import Star from "lucide-solid/icons/star";
import Swords from "lucide-solid/icons/swords";
import Trophy from "lucide-solid/icons/trophy";
import Users from "lucide-solid/icons/users";
import type { JSX } from "solid-js";
import { Dynamic } from "solid-js/web";

type IconComponent = (props: JSX.SvgSVGAttributes<SVGSVGElement>) => JSX.Element;

/** The icons a view can have, by the name stored with the view. */
export const VIEW_ICONS: Readonly<Record<string, IconComponent>> = {
  library: Library,
  star: Star,
  heart: Heart,
  "hard-drive": HardDrive,
  clock: Clock,
  "list-checks": ListChecks,
  bookmark: Bookmark,
  "eye-off": EyeOff,
  "gamepad-2": Gamepad2,
  joystick: Joystick,
  trophy: Trophy,
  crown: Crown,
  flame: Flame,
  sparkles: Sparkles,
  users: Users,
  swords: Swords,
  shield: Shield,
  skull: Skull,
  ghost: Ghost,
  puzzle: Puzzle,
  brain: Brain,
  dices: Dices,
  box: Box,
  rocket: Rocket,
  map: Map,
  mountain: Mountain,
  car: Car,
  archive: Archive,
};

export const VIEW_ICON_NAMES = Object.keys(VIEW_ICONS);

/** A view's icon; unknown names show a bookmark. */
export function ViewIcon(props: { name: string; class?: string }) {
  return (
    <Dynamic
      component={VIEW_ICONS[props.name] ?? Bookmark}
      class={props.class ?? "icon"}
      aria-hidden="true"
    />
  );
}
