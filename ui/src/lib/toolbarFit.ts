/** One toolbar item that may give way when the toolbar is too narrow. */
export interface FitItem {
  id: string;
  width: number;
  /** Goes into the "More" menu when it gives way (else it is just left out, like the count). */
  menu: boolean;
}

export interface FitInput {
  /** Width the toolbar's items may use. */
  available: number;
  /** Space between items. */
  gap: number;
  /** Widths of the items that always stay (the filter box at its smallest). */
  fixed: readonly number[];
  /** Items in the order they give way: the first one goes first. */
  items: readonly FitItem[];
  /** Width of the "More" button, shown once an item went into the menu. */
  moreWidth: number;
}

const total = (widths: readonly number[], gap: number) =>
  widths.reduce((sum, width) => sum + width, 0) + gap * Math.max(0, widths.length - 1);

/** The ids of the items that give way so the rest fits (all of them if even that is too wide). */
export function fitToolbar(input: FitInput): Set<string> {
  const hidden = new Set<string>();
  const fits = () => {
    const shown = input.items.filter((item) => !hidden.has(item.id)).map((item) => item.width);
    const more = input.items.some((item) => item.menu && hidden.has(item.id));
    const widths = [...input.fixed, ...shown, ...(more ? [input.moreWidth] : [])];
    return total(widths, input.gap) <= input.available;
  };
  for (const item of input.items) {
    if (fits()) break;
    hidden.add(item.id);
  }
  return hidden;
}
