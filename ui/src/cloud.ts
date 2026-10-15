// Cloud drives: creating a vault in Dropbox / OneDrive / Google Drive, and
// setting up two-way sync between a vault and a cloud folder. Marrow never
// talks to the cloud itself; it uses the folders those apps keep on disk.

import { documentDir, homeDir } from "@tauri-apps/api/path";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import { api, type CloudLocation, type MirrorStatus } from "./api";
import { ago, h, icon } from "./util";

const PROVIDER_ICON: Record<CloudLocation["provider"], [string, string]> = {
  dropbox: ["dropbox-logo", "#0061fe"],
  gdrive: ["google-drive-logo", "#1fa463"],
  onedrive: ["cloud", "#0a64c8"],
  icloud: ["apple-logo", "var(--ink-78)"],
};

export function providerIcon(provider: CloudLocation["provider"] | null | undefined): HTMLElement {
  const [name, color] = provider ? PROVIDER_ICON[provider] : ["folder-simple", "var(--ink-70)"];
  const el = icon(name);
  el.style.color = color;
  return el;
}

function join(dir: string, name: string): string {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.replace(/[\\/]+$/, "") + sep + name;
}

/** A small centred dialog. Returns a function that closes it. */
function dialog(title: string, body: (Node | null)[], foot: Node[]): () => void {
  const card = h("div.dlg", { role: "dialog", "aria-label": title }, h("h2", null, title), ...body, h("div.dlg-foot", null, ...foot));
  const wrap = h("div.backdrop.dlg-wrap", null, card);
  const close = () => wrap.remove();
  wrap.addEventListener("mousedown", (e) => e.target === wrap && close());
  wrap.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  });
  document.body.append(wrap);
  window.setTimeout(() => card.querySelector<HTMLElement>("input, .where-opt")?.focus(), 0);
  return close;
}

interface Place {
  key: string;
  label: string;
  path: string;
  provider?: CloudLocation["provider"];
  icon?: string;
  /** Opens a folder picker instead of using `path`. */
  pick?: boolean;
}

/** A list of places to pick one from. */
function placeList(places: Place[], onPick: (p: Place) => void | Promise<void>) {
  let current: Place | null = null;
  const list = h("div.where", { role: "radiogroup" });
  const render = () => {
    list.replaceChildren(
      ...places.map((p) =>
        h(
          "button.where-opt",
          {
            role: "radio",
            "aria-checked": String(current?.key === p.key),
            onclick: async () => {
              if (p.pick) {
                const dir = await openDialog({ directory: true, multiple: false, title: "Choose a folder" });
                if (typeof dir !== "string") return;
                p.path = dir;
              }
              current = p;
              render();
              await onPick(p);
            },
          },
          p.provider ? providerIcon(p.provider) : icon(p.icon ?? "folder-simple"),
          h("span.where-text", null, h("span.where-label", null, p.label), p.path ? h("span.where-path", null, p.path) : null),
          h("span.where-check", null, icon("check")),
        ),
      ),
    );
  };
  render();
  return {
    el: list,
    selected: () => current,
    select: (key: string) => {
      const p = places.find((x) => x.key === key);
      if (p) {
        current = p;
        render();
        void onPick(p);
      }
    },
  };
}

/** "Create a vault": a name and a place (this computer, or a cloud drive). */
export async function createVaultDialog(onCreated: (path: string) => void | Promise<void>) {
  const [clouds, docs] = await Promise.all([api.cloudLocations().catch(() => []), documentDir().catch(() => homeDir()).catch(() => "")]);
  const name = h("input.input", { value: "Notes", spellcheck: false, "aria-label": "Vault name" });
  const hint = h("div.dlg-hint");
  const error = h("div.dlg-error", { role: "alert", hidden: true });
  const places: Place[] = [
    ...(docs ? [{ key: "local", label: "This computer", path: join(docs, ""), icon: "desktop" }] : []),
    ...clouds.map((c, i) => ({ key: `cloud${i}`, label: c.label, path: c.path, provider: c.provider })),
    { key: "pick", label: "Somewhere else…", path: "", icon: "folder-open", pick: true },
  ];
  for (const p of places) p.path = p.path.replace(/[\\/]+$/, "");
  const where = placeList(places, (p) => {
    hint.textContent = p.provider
      ? `${p.label} keeps it on your other devices. Open the same folder in Marrow there.`
      : p.key === "local"
        ? "Only on this computer. You can turn on sync with a cloud folder later in Settings."
        : "";
  });
  const create = h("button.btn.btn-primary", null, icon("plus"), "Create vault");
  const close = dialog(
    "Create a vault",
    [
      h("div.field", null, h("label", null, "Name"), name),
      h("div.field", null, h("label", null, "Keep it in"), where.el),
      hint,
      clouds.length ? null : h("div.dlg-hint", null, "No Dropbox, OneDrive or Google Drive folder was found. If you use one, install its desktop app and it will show up here."),
      error,
    ],
    [h("button.btn.btn-secondary", { onclick: () => close() }, "Cancel"), create],
  );
  where.select(clouds.length ? "cloud0" : "local");
  const go = async () => {
    const p = where.selected();
    if (!p?.path) {
      error.hidden = false;
      error.textContent = "Pick where to keep the vault.";
      return;
    }
    try {
      const path = await api.createVault(p.path, name.value);
      close();
      await onCreated(path);
    } catch (e) {
      error.hidden = false;
      error.textContent = String(e);
    }
  };
  create.addEventListener("click", () => void go());
  name.addEventListener("keydown", (e) => e.key === "Enter" && void go());
  name.select();
}

/** "Sync with a cloud folder": pick a cloud drive (or any folder). */
export async function setupSyncDialog(vaultName: string, onDone: () => void) {
  const clouds = await api.cloudLocations().catch(() => []);
  const note = h("div.dlg-hint");
  const error = h("div.dlg-error", { role: "alert", hidden: true });
  const places: Place[] = [
    ...clouds.map((c, i) => ({ key: `cloud${i}`, label: c.label, path: join(c.path, vaultName), provider: c.provider })),
    { key: "pick", label: "Choose a folder…", path: "", icon: "folder-open", pick: true },
  ];
  let target = "";
  const where = placeList(places, async (p) => {
    target = p.path;
    note.textContent = "";
    if (!target) return;
    const n = await api.mirrorPreview(target).catch(() => 0);
    if (where.selected()?.path !== target) return;
    note.textContent =
      n > 0
        ? `That folder already has ${n} ${n === 1 ? "file" : "files"}. They'll be combined with this vault — nothing is deleted, and notes that differ are merged.`
        : "Everything in this vault will be copied there, and kept in sync both ways from then on.";
  });
  const start = h("button.btn.btn-primary", null, icon("arrows-clockwise"), "Start syncing");
  const close = dialog(
    "Sync with a cloud folder",
    [
      h(
        "p.dlg-lede",
        null,
        "Marrow keeps this vault and a folder in your cloud drive the same, both ways. Your cloud app carries it to your other devices.",
      ),
      clouds.length ? null : h("div.dlg-hint", null, "No Dropbox, OneDrive or Google Drive folder was found on this computer. Install its desktop app, or choose any folder."),
      where.el,
      note,
      error,
    ],
    [h("button.btn.btn-secondary", { onclick: () => close() }, "Cancel"), start],
  );
  if (clouds.length) where.select("cloud0");
  start.addEventListener("click", async () => {
    if (!target) {
      error.hidden = false;
      error.textContent = "Pick a folder to sync with.";
      return;
    }
    try {
      await api.setMirror(target);
      close();
      onDone();
    } catch (e) {
      error.hidden = false;
      error.textContent = String(e);
    }
  });
}

/** One line describing sync, e.g. "Synced with Dropbox 2 min ago". */
export function syncLine(s: MirrorStatus): string {
  if (s.message) return s.message;
  if (s.syncing && !s.lastSync) return `Syncing with ${s.label}…`;
  if (!s.lastSync) return `Syncing with ${s.label}`;
  return `Synced with ${s.label} ${ago(s.lastSync)}`;
}
