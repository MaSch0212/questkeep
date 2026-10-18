import type { JSX } from "solid-js";

/** Inline icons (no icon dependency). They inherit the text color and are decorative. */
interface IconProps {
  class?: string;
}

function Svg(props: IconProps & { children: JSX.Element }) {
  return (
    <svg
      class={`icon ${props.class ?? ""}`.trim()}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {props.children}
    </svg>
  );
}

export function GridIcon(props: IconProps) {
  return (
    <Svg class={props.class}>
      <rect x="2" y="2" width="5" height="5" rx="1" />
      <rect x="9" y="2" width="5" height="5" rx="1" />
      <rect x="2" y="9" width="5" height="5" rx="1" />
      <rect x="9" y="9" width="5" height="5" rx="1" />
    </Svg>
  );
}

export function ListIcon(props: IconProps) {
  return (
    <Svg class={props.class}>
      <path d="M5 4h9M5 8h9M5 12h9" />
      <path d="M2 4h.01M2 8h.01M2 12h.01" />
    </Svg>
  );
}

export function DownloadIcon(props: IconProps) {
  return (
    <Svg class={props.class}>
      <path d="M8 2v8M4.5 6.5 8 10l3.5-3.5M3 13h10" />
    </Svg>
  );
}

export function EyeIcon(props: IconProps) {
  return (
    <Svg class={props.class}>
      <path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8Z" />
      <circle cx="8" cy="8" r="2" />
    </Svg>
  );
}

export function EyeOffIcon(props: IconProps) {
  return (
    <Svg class={props.class}>
      <path d="M2 2l12 12" />
      <path d="M6.6 4a6 6 0 0 1 1.4-.5C12 3.5 14.5 8 14.5 8a11 11 0 0 1-1.7 2.2M9.4 11.9A6 6 0 0 1 8 12.5C4 12.5 1.5 8 1.5 8a11 11 0 0 1 2.2-2.7" />
    </Svg>
  );
}
