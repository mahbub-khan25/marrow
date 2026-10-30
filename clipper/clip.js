// Sending a page to Marrow, which must be open on this computer. Marrow
// listens on 127.0.0.1 only and answers only browser extensions.

const MARROW = "http://127.0.0.1:53683";
export const t = (key, ...subs) => chrome.i18n.getMessage(key, subs) || key;

/** Asks Marrow something; throws with words for the person. */
async function call(path, body = {}) {
  let r;
  try {
    r = await fetch(MARROW + path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch {
    throw new Error(t("notRunning"));
  }
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j.error || r.statusText);
  return j;
}

/** `{vault}` (null when Marrow has no vault open); throws when Marrow isn't running. */
export const hello = () => call("/hello");

export const openInMarrow = (path) => call("/open", { path });

/** The page in the tab: its address, HTML and selected part (HTML, or ""). */
export async function readTab(tabId) {
  let res;
  try {
    [res] = await chrome.scripting.executeScript({
      target: { tabId },
      func: () => {
        const sel = window.getSelection();
        let selection = "";
        if (sel && sel.rangeCount && !sel.isCollapsed) {
          const box = document.createElement("div");
          for (let i = 0; i < sel.rangeCount; i++) box.appendChild(sel.getRangeAt(i).cloneContents());
          selection = box.innerHTML;
        }
        return { url: location.href, html: document.documentElement.outerHTML, selection };
      },
    });
  } catch {
    throw new Error(t("cantRead"));
  }
  if (!res || !res.result) throw new Error(t("cantRead"));
  return res.result;
}

/** Clips the tab's page (or just its selection) into Marrow: `{path, title}`. */
export async function clipTab(tabId, selectionOnly) {
  const page = await readTab(tabId);
  return call("/clip", {
    url: page.url,
    html: page.html,
    selection: selectionOnly ? page.selection : null,
  });
}
