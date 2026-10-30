// Right-click "Clip to Marrow" on a page or a selection.
import { clipTab, t } from "./clip.js";

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: "page", title: t("menuPage"), contexts: ["page", "link", "image"] });
    chrome.contextMenus.create({ id: "selection", title: t("menuSelection"), contexts: ["selection"] });
  });
});

function badge(tabId, text, color, title) {
  chrome.action.setBadgeBackgroundColor({ tabId, color });
  chrome.action.setBadgeText({ tabId, text });
  chrome.action.setTitle({ tabId, title });
  setTimeout(() => {
    chrome.action.setBadgeText({ tabId, text: "" });
    chrome.action.setTitle({ tabId, title: t("actionTitle") });
  }, 4000);
}

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (!tab || tab.id === undefined) return;
  try {
    const r = await clipTab(tab.id, info.menuItemId === "selection");
    badge(tab.id, "✓", "#0A7EA4", t("saved", r.title));
  } catch (e) {
    badge(tab.id, "!", "#B3261E", e.message);
  }
});
