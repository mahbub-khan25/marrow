// Pasting and dropping files into a note: they're saved in the vault's
// "attachments" folder and linked where they landed (images as ![[embeds]]).

import { api, displayName } from "./api";

export const ATTACHMENT_FOLDER = "attachments";

const IMAGE = /\.(png|jpe?g|gif|webp|svg|avif|bmp)$/i;

/** The files in a paste or drop, if any. */
export function filesIn(data: DataTransfer | null): File[] {
  if (!data) return [];
  const files = Array.from(data.files ?? []);
  if (files.length) return files;
  // Some apps put a pasted image only in `items`.
  return Array.from(data.items ?? [])
    .filter((i) => i.kind === "file")
    .map((i) => i.getAsFile())
    .filter((f): f is File => f !== null);
}

/** Whether a drag carries files from outside (not text being moved). */
export function draggingFiles(e: DragEvent): boolean {
  return Array.from(e.dataTransfer?.types ?? []).includes("Files");
}

const stamp = () => {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
};

/** A screenshot pasted from the clipboard is called "image.png"; give it a better name. */
function nameFor(f: File, pasted: boolean): string {
  const ext = (/\.([a-z0-9]+)$/i.exec(f.name)?.[1] ?? f.type.split("/")[1] ?? "png").replace("jpeg", "jpg").replace("svg+xml", "svg");
  if (!f.name || (pasted && /^image\.\w+$/i.test(f.name))) return `Pasted image ${stamp()}.${ext}`;
  return f.name;
}

/** Markdown for a saved file: `![[image.png]]` or `[[file.pdf]]`. */
async function linkFor(path: string, source: string): Promise<string> {
  const file = path.split("/").pop()!;
  // The file name alone, unless another file of that name would win.
  const found = await api.resolveLink(file, source).catch(() => null);
  const link = found === path ? file : path;
  return /\.md$/i.test(file) ? `[[${displayName(path)}]]` : IMAGE.test(file) ? `![[${link}]]` : `[[${link}]]`;
}

/**
 * The clipboard's image, saved, as Markdown — for webviews that don't put
 * pasted images in the paste event (Linux). Null if there's no image.
 */
export async function pasteClipboardImage(source: string): Promise<string | null> {
  const path = await api.saveClipboardImage(`Pasted image ${stamp()}.png`, ATTACHMENT_FOLDER);
  return path ? linkFor(path, source) : null;
}

/**
 * Saves the files and returns the Markdown to insert, e.g. `![[photo.png]]`.
 * `source` is the note they go into (for the shortest link that still works).
 */
export async function saveFiles(files: File[], source: string, pasted: boolean): Promise<string> {
  const out: string[] = [];
  for (const f of files) {
    const bytes = new Uint8Array(await f.arrayBuffer());
    const path = await api.saveAttachment(bytes, nameFor(f, pasted), ATTACHMENT_FOLDER);
    out.push(await linkFor(path, source));
  }
  return out.join(out.some((m) => m.startsWith("!")) ? "\n" : " ");
}
