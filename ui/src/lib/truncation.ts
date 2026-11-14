/** Shows an element's text as its tooltip only while the text is cut off (ellipsis). */
export function titleWhenCut(element: HTMLElement): void {
  if (element.scrollWidth > element.clientWidth) {
    element.dataset.tooltip = element.textContent ?? "";
  } else delete element.dataset.tooltip;
}
