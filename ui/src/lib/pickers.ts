import { open } from "@tauri-apps/plugin-dialog";

async function pickFile(title: string, filters: { name: string; extensions: string[] }[]) {
  const result = await open({ multiple: false, directory: false, title, filters });
  return typeof result === "string" ? result : null;
}

export function pickExecutable(): Promise<string | null> {
  return pickFile("Choose the game executable", [
    { name: "Executable", extensions: ["exe", "bat", "cmd", "lnk"] },
    { name: "All files", extensions: ["*"] },
  ]);
}

export function pickImage(): Promise<string | null> {
  return pickFile("Choose a cover image", [
    { name: "Image", extensions: ["jpg", "jpeg", "png", "webp", "gif"] },
  ]);
}

export async function pickDirectory(): Promise<string | null> {
  const result = await open({ multiple: false, directory: true, title: "Choose a folder" });
  return typeof result === "string" ? result : null;
}
