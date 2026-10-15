// Quick switcher (Ctrl+O): type part of a note's name, Enter opens it.
// Shift+Enter (or the last row) creates a note with that name.

import { api, type FileHit } from "./api";

interface SwitcherOptions {
  source: () => string | undefined;
  /** `newTab` is true when Ctrl+Enter (or Ctrl-click) was used. */
  onPick: (path: string, newTab: boolean) => void;
  onCreate: (name: string) => void | Promise<void>;
}

export function openSwitcher(opts: SwitcherOptions) {
  const dlg = document.getElementById("switcher") as HTMLDialogElement;
  const input = dlg.querySelector("input")!;
  const list = dlg.querySelector("ul")!;
  let hits: FileHit[] = [];
  let active = 0;
  let seq = 0;

  const render = () => {
    list.replaceChildren();
    hits.forEach((h, i) => {
      const li = document.createElement("li");
      li.setAttribute("role", "option");
      li.setAttribute("aria-selected", String(i === active));
      const name = document.createElement("span");
      name.className = "sw-name";
      name.textContent = h.name;
      const folder = document.createElement("span");
      folder.className = "sw-folder";
      folder.textContent = h.folder;
      li.append(name, folder);
      li.addEventListener("mousedown", (e) => {
        e.preventDefault();
        pick(i, e.ctrlKey || e.metaKey);
      });
      list.append(li);
    });
    const q = input.value.trim();
    if (q) {
      const li = document.createElement("li");
      li.setAttribute("role", "option");
      li.className = "sw-create";
      li.setAttribute("aria-selected", String(active === hits.length));
      li.textContent = `Create note “${q}”`;
      li.addEventListener("mousedown", (e) => {
        e.preventDefault();
        create();
      });
      list.append(li);
    }
    list.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  };

  const search = async () => {
    const mine = ++seq;
    const found = await api.findFiles(input.value.trim(), opts.source(), 50);
    if (mine !== seq) return;
    hits = found.filter((h) => h.isNote);
    active = 0;
    render();
  };

  const close = () => {
    dlg.close();
    input.removeEventListener("input", onInput);
    input.removeEventListener("keydown", onKey);
  };
  const pick = (i: number, newTab = false) => {
    const h = hits[i];
    close();
    if (h) opts.onPick(h.path, newTab);
  };
  const create = () => {
    const q = input.value.trim();
    close();
    if (q) void opts.onCreate(q);
  };

  const onInput = () => void search();
  const onKey = (e: KeyboardEvent) => {
    const count = hits.length + (input.value.trim() ? 1 : 0);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      active = Math.min(active + 1, count - 1);
      render();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      active = Math.max(active - 1, 0);
      render();
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey || active === hits.length) create();
      else pick(active, e.ctrlKey || e.metaKey);
    }
  };

  input.value = "";
  input.addEventListener("input", onInput);
  input.addEventListener("keydown", onKey);
  dlg.addEventListener("close", () => close(), { once: true });
  dlg.showModal();
  input.focus();
  void search();
}
