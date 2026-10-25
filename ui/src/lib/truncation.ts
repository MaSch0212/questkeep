/** Shows an element's text as its tooltip only while the text is cut off (ellipsis). */
export function titleWhenCut(element: HTMLElement): void {
  if (element.scrollWidth > element.clientWidth) element.title = element.textContent ?? "";
  else element.removeAttribute("title");
}
