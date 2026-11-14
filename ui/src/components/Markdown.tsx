import { createMemo } from "solid-js";
import { renderMarkdown } from "../lib/markdown";
import { openLink } from "./settings/openLink";

/** Links in rendered markdown open in the user's browser. */
function onClick(event: MouseEvent) {
  const link = (event.target as Element | null)?.closest?.("a[href]");
  if (!(link instanceof HTMLAnchorElement)) return;
  event.preventDefault();
  if (/^https?:/i.test(link.href)) void openLink(link.href);
}

/** Renders trusted-source markdown (release notes); links open in the user's browser. */
export function Markdown(props: { source: string; class?: string }) {
  const html = createMemo(() => renderMarkdown(props.source));
  return (
    // oxlint-disable-next-line no-static-element-interactions -- delegated link clicks
    <div class={`markdown ${props.class ?? ""}`} innerHTML={html()} onClick={onClick} />
  );
}
