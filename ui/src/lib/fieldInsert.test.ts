import { describe, expect, it, vi } from "vitest";
import { insertAtCursor } from "./fieldInsert";

describe("insertAtCursor", () => {
  it("inserts at the cursor, replaces a selection and tells listeners", () => {
    const field = document.createElement("input");
    document.body.append(field);
    field.value = "abcd";
    field.setSelectionRange(2, 2);
    const heard = vi.fn<() => void>();
    field.addEventListener("input", heard);
    insertAtCursor(field, "X");
    expect(field.value).toBe("abXcd");
    expect(field.selectionStart).toBe(3);
    field.setSelectionRange(0, 2);
    insertAtCursor(field, "{GameName}");
    expect(field.value).toBe("{GameName}Xcd");
    expect(heard).toHaveBeenCalledTimes(2);
    field.remove();
  });

  it("appends to a field that never had a cursor", () => {
    const field = document.createElement("textarea");
    document.body.append(field);
    field.value = "hi ";
    insertAtCursor(field, "there");
    expect(field.value).toContain("there");
    field.remove();
  });
});
