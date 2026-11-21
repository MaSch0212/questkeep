/**
 * Shows the focus ring only after the user pressed Tab. The browser's own `:focus-visible` also
 * turns on when a modifier such as Shift is pressed while something is focused, which put a ring
 * on buttons and list rows. The stylesheet scopes every `:focus-visible` rule to
 * `html[data-keyboard-focus]`, which this sets on Tab and clears on any pointer press.
 */
export function installFocusRing(doc: Document = document): () => void {
  const root = doc.documentElement;
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Tab") root.setAttribute("data-keyboard-focus", "");
  };
  const onPointer = () => root.removeAttribute("data-keyboard-focus");
  doc.addEventListener("keydown", onKeyDown, true);
  doc.addEventListener("pointerdown", onPointer, true);
  return () => {
    doc.removeEventListener("keydown", onKeyDown, true);
    doc.removeEventListener("pointerdown", onPointer, true);
    root.removeAttribute("data-keyboard-focus");
  };
}
