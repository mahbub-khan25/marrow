// Cloud drives: creating a vault in Dropbox / OneDrive / Google Drive, and
// setting up two-way sync between a vault and a cloud folder (kept on disk by
// that cloud's app) or a cloud service Marrow talks to directly (WebDAV).

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

export function providerIcon(provider: CloudLocation["provider"] | "webdav" | null | undefined): HTMLElement {
  const [name, color] = provider === "webdav" ? ["hard-drives", "var(--color-accent)"] : provider ? PROVIDER_ICON[provider] : ["folder-simple", "var(--ink-70)"];
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
  provider?: CloudLocation["provider"] | "webdav";
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

/** Sync setup: a cloud folder on this computer, or a server Marrow talks to directly. */
export async function setupSyncDialog(vaultName: string, onDone: () => void, mobile = false) {
  // Phones and tablets can't use another app's folder, only services Marrow talks to.
  const clouds = mobile ? [] : await api.cloudLocations().catch(() => []);
  const note = h("div.dlg-hint");
  const error = h("div.dlg-error", { role: "alert", hidden: true });
  const field = (label: string, input: HTMLInputElement) => h("label.dlg-field", null, h("span", null, label), input);
  const url = h("input.input", { type: "url", placeholder: "https://cloud.example.com/remote.php/dav/files/you/Marrow", autocomplete: "off", spellcheck: false }) as HTMLInputElement;
  const user = h("input.input", { type: "text", autocomplete: "username", spellcheck: false, autocapitalize: "off" }) as HTMLInputElement;
  const pass = h("input.input", { type: "password", autocomplete: "current-password" }) as HTMLInputElement;
  const server = h(
    "div.dlg-server",
    { hidden: true },
    field("Server and folder", url),
    h("div.dlg-row", null, field("User name", user), field("Password", pass)),
    h(
      "div.dlg-hint",
      null,
      "Nextcloud: create an app password in Settings › Security, and use the WebDAV address from Files › Settings with a folder name added, such as …/Marrow. Synology, pCloud and Koofr show their WebDAV address in their settings.",
    ),
  );
  // End-to-end encryption: on, and (for a place already encrypted) joining it.
  const encOn = h("input", { type: "checkbox" }) as HTMLInputElement;
  const pp1 = h("input.input", { type: "password", autocomplete: "new-password" }) as HTMLInputElement;
  const pp2 = h("input.input", { type: "password", autocomplete: "new-password" }) as HTMLInputElement;
  const pp2Field = field("Type it again", pp2);
  const encHint = h("div.dlg-hint");
  const encFields = h("div.dlg-enc-fields", { hidden: true }, h("div.dlg-row", null, field("Sync passphrase", pp1), pp2Field), encHint);
  const encBox = h(
    "div.dlg-enc",
    null,
    h(
      "label.dlg-check",
      null,
      encOn,
      h("span", null, h("b", null, "Encrypt everything before it leaves this device. "), "The cloud stores only scrambled data, even the file names."),
    ),
    encFields,
  );
  let joining = false;
  const setEnc = (mode: "off" | "new" | "join" | "unavailable") => {
    joining = mode === "join";
    encOn.disabled = mode === "join" || mode === "unavailable";
    encOn.checked = mode === "new" || mode === "join" ? true : mode === "unavailable" ? false : encOn.checked;
    encFields.hidden = !encOn.checked;
    pp2Field.hidden = joining;
    encHint.textContent = joining
      ? "This place holds a vault Marrow encrypted. Enter its sync passphrase."
      : mode === "unavailable"
        ? "Encrypted sync needs an empty folder, or one Marrow already encrypted."
        : "Use the same passphrase on your other devices. Marrow can't recover it: if it's lost, the copy in the cloud can't be read (the notes on your devices are fine).";
    // Unavailable: explain why, without passphrase boxes.
    const boxes = encFields.firstElementChild as HTMLElement;
    boxes.hidden = mode === "unavailable";
    if (mode === "unavailable") encFields.hidden = false;
  };
  encOn.addEventListener("change", () => setEnc(encOn.checked ? "new" : "off"));
  const places: Place[] = [
    ...clouds.map((c, i) => ({ key: `cloud${i}`, label: c.label, path: join(c.path, vaultName), provider: c.provider })),
    { key: "webdav", label: "Nextcloud, Synology or another WebDAV server", path: "", provider: "webdav" as const },
    ...(mobile ? [] : [{ key: "pick", label: "Choose a folder…", path: "", icon: "folder-open", pick: true }]),
  ];
  let target = "";
  const where = placeList(places, async (p) => {
    target = p.path;
    note.textContent = "";
    error.hidden = true;
    server.hidden = p.key !== "webdav";
    setEnc("off");
    if (p.key === "webdav") {
      note.textContent = "Marrow checks the sign-in before it starts. Without encryption, notes stay plain Markdown files in that folder.";
      window.setTimeout(() => url.focus(), 0);
      return;
    }
    if (!target) return;
    const [n, enc] = await Promise.all([api.mirrorPreview(target).catch(() => 0), api.folderEncrypted(target).catch(() => false)]);
    if (where.selected()?.path !== target) return;
    if (enc) setEnc("join");
    else if (n > 0) setEnc("unavailable");
    note.textContent = enc
      ? "That folder holds this vault encrypted. Your notes will be combined with it."
      : n > 0
        ? `That folder already has ${n} ${n === 1 ? "file" : "files"}. They'll be combined with this vault — nothing is deleted, and notes that differ are merged.`
        : "Everything in this vault will be copied there, and kept in sync both ways from then on.";
  });
  const label = h("span", null, "Start syncing");
  const start = h("button.btn.btn-primary", null, icon("arrows-clockwise"), label) as HTMLButtonElement;
  const close = dialog(
    "Sync this vault",
    [
      h(
        "p.dlg-lede",
        null,
        mobile
          ? "Keep this vault the same on all your devices, through your own server."
          : "Keep this vault the same everywhere: through a folder your cloud app syncs (Dropbox, OneDrive, Google Drive, iCloud), or your own server.",
      ),
      clouds.length || mobile ? null : h("div.dlg-hint", null, "No Dropbox, OneDrive or Google Drive folder was found on this computer. Install its desktop app, or choose any folder."),
      where.el,
      server,
      encBox,
      note,
      error,
    ],
    [h("button.btn.btn-secondary", { onclick: () => close() }, "Cancel"), start],
  );
  where.select(clouds.length ? "cloud0" : "webdav");
  const fail = (msg: string) => {
    error.hidden = false;
    error.textContent = msg;
  };
  /** The passphrase to use, or undefined when not encrypting; throws a message when it isn't right yet. */
  const passphrase = (): string | undefined => {
    if (!encOn.checked) return undefined;
    if (pp1.value.length < 8) throw "Use a sync passphrase of at least 8 characters.";
    if (!joining && pp1.value !== pp2.value) throw "The two passphrases don't match.";
    return pp1.value;
  };
  start.addEventListener("click", async () => {
    error.hidden = true;
    let phrase: string | undefined;
    try {
      phrase = passphrase();
    } catch (e) {
      return fail(String(e));
    }
    if (where.selected()?.key === "webdav") {
      if (!url.value.trim() || !user.value.trim() || !pass.value) return fail("Fill in the server address, user name and password.");
      start.disabled = true;
      label.textContent = "Checking…";
      try {
        const info = await api.webdavCheck(url.value.trim(), user.value.trim(), pass.value);
        if (info.encrypted && !joining) {
          setEnc("join");
          pp1.value = "";
          return fail("That server folder holds an encrypted vault. Enter its sync passphrase, then start again.");
        }
        if (!info.encrypted && info.files > 0 && phrase) return fail("Encrypted sync needs an empty folder. That one already has files: add a new folder name to the address.");
        if (phrase) label.textContent = "Setting up encryption…";
        await api.setCloudSync(url.value.trim(), user.value.trim(), pass.value, phrase);
        close();
        onDone();
      } catch (e) {
        fail(String(e));
      } finally {
        start.disabled = false;
        label.textContent = "Start syncing";
      }
      return;
    }
    if (!target) return fail("Pick a folder to sync with.");
    try {
      await api.setMirror(target, phrase);
      close();
      onDone();
    } catch (e) {
      fail(String(e));
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
