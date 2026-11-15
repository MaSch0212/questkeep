import { fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { Modal } from "./Modal";

describe("Modal", () => {
  it("closes on Escape, a click outside and the close button", async () => {
    const onClose = vi.fn<() => void>();
    render(() => (
      <Modal title="Plain" onClose={onClose}>
        <p>Body</p>
      </Modal>
    ));
    const dialog = screen.getByRole("dialog", { name: "Plain" });
    await fireEvent.keyDown(dialog, { key: "Escape" });
    await fireEvent.mouseDown(dialog.parentElement as HTMLElement);
    await fireEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(onClose).toHaveBeenCalledTimes(3);
  });

  it("does not close from a press inside the dialog", async () => {
    const onClose = vi.fn<() => void>();
    render(() => (
      <Modal title="Plain" onClose={onClose}>
        <p>Body</p>
      </Modal>
    ));
    await fireEvent.mouseDown(screen.getByText("Body"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("asks first while there are unsaved changes, and not once they are gone", async () => {
    const onClose = vi.fn<() => void>();
    const onSave = vi.fn<() => void>();
    const [dirty, setDirty] = createSignal(true);
    render(() => (
      <Modal title="Edit" onClose={onClose} dirty={dirty} onSave={onSave}>
        <p>Body</p>
      </Modal>
    ));
    const dialog = screen.getByRole("dialog", { name: "Edit" });

    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(await screen.findByRole("dialog", { name: "Unsaved changes" })).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();

    // The close button asks too; Save hands over to the dialog's own save.
    await fireEvent.click(await screen.findByRole("button", { name: "Keep editing" }));
    await fireEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    await fireEvent.click(await screen.findByRole("button", { name: "Save" }));
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog", { name: "Unsaved changes" })).toBeNull();

    setDirty(false);
    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
