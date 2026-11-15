/**
 * Inserts text at the cursor of a text field (replacing the selection) and puts the cursor after
 * it. Fires `input`, so a field whose state follows its input events updates.
 */
export function insertAtCursor(field: HTMLInputElement | HTMLTextAreaElement, text: string) {
  const start = field.selectionStart ?? field.value.length;
  const end = field.selectionEnd ?? start;
  field.setRangeText(text, start, end, "end");
  field.dispatchEvent(new Event("input", { bubbles: true }));
  field.focus();
}
