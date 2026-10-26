// "How Marrow works": every place in the app in one plain sentence, and the
// handful of keys worth knowing. Opened from Help in the rail.

import { h, icon, kbd } from "./util";

const PLACES: [string, string, string][] = [
  ["files", "Files", "All your notes and folders. Right-click one for more, such as Rename, Move or Version history."],
  ["magnifying-glass", "Search", "Finds words in every note. Spelling mistakes in Bangla are forgiven."],
  ["hash", "Tags", "Notes grouped by the #tags you write in them."],
  ["check-square", "Tasks", "Every to-do from every note in one list, with what's due."],
  ["calendar-dots", "Calendar", "Your daily notes by month. Click a day to open or start its note."],
  ["sparkle", "Ask", "Ask a question and get an answer from your own notes."],
  ["sun", "Today", "Opens today's note, the place for quick thoughts and to-dos."],
  ["graph", "Graph", "A map of how your notes link to each other."],
  ["squares-four", "Canvas", "A whiteboard for arranging notes, cards and drawings."],
  ["command", "Commands", "Search notes and run any command by typing its name."],
  ["gear-six", "Settings", "Text size, theme, language, sync and more."],
];

export function openHelp() {
  const keys: [string, string][] = [
    [kbd("K"), "Find a note, or run a command"],
    [kbd("N"), "New note"],
    [kbd("D"), "Today's note"],
    [kbd("E"), "Switch between reading and editing the Markdown"],
    [kbd("F", true), "Search inside every note"],
    ["[[", "Link to another note while you type"],
    ["/", "On an empty line: headings, lists, tables and more"],
  ];
  const close = () => {
    wrap.remove();
    document.removeEventListener("keydown", onKey, true);
    document.documentElement.toggleAttribute("data-modal", false);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  };
  const card = h(
    "div.dlg.help",
    { role: "dialog", "aria-label": "How Marrow works" },
    h("div.help-head", null, h("h2", null, "How Marrow works"), h("button.icon-btn", { title: "Close (Esc)", "aria-label": "Close", onclick: close }, icon("x"))),
    h(
      "p.dlg-lede",
      null,
      "Every note is a plain text file in a folder on your computer, so it's always yours. Marrow saves as you type; there's no Save button.",
    ),
    h("h3.help-sec", null, "Where things are"),
    h(
      "ul.help-places",
      null,
      ...PLACES.map(([ic, name, what]) => h("li", null, h("span.hp-icon", null, icon(ic)), h("span", null, h("b", null, name), h("span.hp-what", null, what)))),
    ),
    h("h3.help-sec", null, "Handy keys"),
    h("ul.help-keys", null, ...keys.map(([k, what]) => h("li", null, h("kbd", null, k), h("span", null, what)))),
    h(
      "div.dlg-foot",
      null,
      h("button.btn.btn-primary", { onclick: close }, "Got it"),
    ),
  );
  const wrap = h("div.backdrop.dlg-wrap", null, card);
  wrap.addEventListener("mousedown", (e) => e.target === wrap && close());
  document.addEventListener("keydown", onKey, true);
  document.body.append(wrap);
  document.documentElement.toggleAttribute("data-modal", true);
}
