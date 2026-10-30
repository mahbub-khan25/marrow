import { clipTab, hello, openInMarrow, readTab, t } from "./clip.js";

const $ = (id) => document.getElementById(id);
for (const el of document.querySelectorAll("[data-i18n]")) el.textContent = t(el.dataset.i18n);
document.documentElement.lang = chrome.i18n.getUILanguage();

function say(text, kind = "") {
  $("status").textContent = text;
  $("status").className = kind;
}

const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });

async function start() {
  say(t("looking"));
  try {
    const h = await hello();
    if (!h.vault) return say(t("noVault"), "error");
    say(t("savingTo", h.vault));
  } catch (e) {
    return say(e.message, "error");
  }
  $("actions").hidden = false;
  $("page").focus();
  // Offer "Clip the selection" when something is selected.
  try {
    const page = await readTab(tab.id);
    $("selection").hidden = !page.selection;
  } catch (e) {
    say(e.message, "error");
    $("page").disabled = true;
  }
}

async function clip(selectionOnly) {
  for (const b of document.querySelectorAll("button")) b.disabled = true;
  say(t("clipping"));
  try {
    const r = await clipTab(tab.id, selectionOnly);
    say(t("saved", r.title), "ok");
    $("actions").hidden = true;
    $("done").hidden = false;
    $("open").disabled = false;
    $("open").focus();
    $("open").onclick = async () => {
      await openInMarrow(r.path).catch(() => {});
      window.close();
    };
  } catch (e) {
    say(e.message, "error");
    for (const b of document.querySelectorAll("button")) b.disabled = false;
  }
}

$("page").onclick = () => clip(false);
$("selection").onclick = () => clip(true);
start();
