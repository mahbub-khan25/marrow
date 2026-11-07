# Marrow Clipper: store listings

Everything the Chrome Web Store, Microsoft Edge Add-ons and Firefox Add-ons
(AMO) ask for, ready to paste. The images are in this folder. The packages
come from `python3 clipper/pack.py <version> <folder>` (and are attached to
every release):

- `Marrow_Clipper_<version>_chrome.zip`: Chrome Web Store and Edge Add-ons.
- `Marrow_Clipper_<version>_firefox.zip`: Firefox Add-ons.

## The same in every store

**Name:** Marrow Clipper

**Short description** (132 characters at most; it's the manifest's):

> Save web pages and selections as notes in Marrow, the Markdown notes app. Marrow must be open on this computer.

**Full description:**

> Marrow Clipper saves the page you're reading, or just the part you selected, as a note in Marrow, the free and open-source Markdown notes app for Windows, macOS and Linux.
>
> • Click the Marrow button (or press Alt+Shift+M), then Clip this page or Clip the selection.
> • Or right-click a page or a selection and choose Clip to Marrow.
>
> The article is picked out of the page and turned into clean Markdown. Menus, ads and footers stay behind. The note lands in the Clippings folder of your notes, with the page's address and the date on top, ready to link, tag and search like any other note.
>
> Private by design: the extension reads a page only when you clip it, and sends it only to the Marrow app on your own computer (127.0.0.1). Nothing is sent to us or to anyone else. No account, no ads, no tracking.
>
> Marrow must be open on the same computer. Get it free at https://www.cicadama.com/marrow/

**Website:** https://www.cicadama.com/marrow/

**Support:** https://github.com/mahbub-khan25/marrow/issues

**Privacy policy:** https://www.cicadama.com/marrow/privacy/

**Language:** English (the extension is also in Bangla).

**Price:** free.

**Images in this folder:**

| File | Size | Chrome | Edge | Firefox |
|---|---|---|---|---|
| `screenshot-1-clip.png` | 1280×800 | ✓ | ✓ | ✓ |
| `screenshot-2-saved.png` | 1280×800 | ✓ | ✓ | ✓ |
| `screenshot-3-in-marrow.png` | 1280×800 | ✓ | ✓ | ✓ |
| `promo-small-440x280.png` | 440×280 | small promo tile | small promo tile | |
| `promo-marquee-1400x560.png` | 1400×560 | marquee (optional) | large promo tile (optional) | |
| `logo-300x300.png` | 300×300 | | extension logo | |

The 128px icon comes from the package itself.

## Notes for reviewers (paste into every store's "notes to reviewer" box)

> This extension sends pages to the Marrow desktop app, which must be running on the same computer. Without it, the popup says "Marrow isn't open on this computer".
>
> To test:
> 1. Install Marrow (free, open source) from https://github.com/mahbub-khan25/marrow/releases/latest (Windows installer, macOS .dmg, or Linux AppImage/.deb/.rpm).
> 2. Open Marrow and create or open a notes folder (choose "Create a vault" on the welcome screen).
> 3. Open any article in the browser, click the Marrow Clipper button and choose "Clip this page". The note appears in Marrow's "Clippings" folder.
>
> Marrow listens only on 127.0.0.1:53683 and only accepts requests whose Origin is a browser extension. The extension contains no remote code and no minified or generated code; its source is at https://github.com/mahbub-khan25/marrow/tree/main/clipper

## Chrome Web Store

Where: https://chrome.google.com/webstore/devconsole (one-time US$5 registration).

- **Category:** Productivity › Workflow & Planning (or the closest productivity category offered).
- **Single purpose:**
  > Save the current web page, or the selected part of it, as a note in the Marrow notes app on the same computer.
- **Permission justifications:**
  - `activeTab`: Reads the current tab only when the user clicks the extension's button, uses its shortcut or chooses it from the right-click menu, so the page can be clipped.
  - `scripting`: Runs one small function in that tab to copy the page's HTML, address and the user's selection. Nothing is injected or changed on the page.
  - `contextMenus`: Adds "Clip this page to Marrow" and "Clip the selection to Marrow" to the right-click menu.
  - Host permission `http://127.0.0.1:53683/*`: Sends the clipped page to the Marrow desktop app on the user's own computer. No other host is contacted.
- **Remote code:** No, I am not using remote code.
- **Data usage:** tick **Website content** only (the page the user chooses to clip). Explain if asked: it goes only to the Marrow app on the same computer. Then tick all three certifications: not sold to third parties; not used or transferred for purposes unrelated to the single purpose; not used to determine creditworthiness or for lending.
- **Visibility:** Public.

## Microsoft Edge Add-ons

Where: https://partner.microsoft.com/dashboard/microsoftedge (free registration as an individual).

- Upload the same `_chrome.zip`.
- **Category:** Productivity.
- **Does the extension access, collect or transmit personal information?** No personal information: it sends only the page the user chooses to clip, to the Marrow app on the same computer. Give the privacy policy URL.
- **Extension logo:** `logo-300x300.png`. Screenshots and promo tiles as above.
- Paste the reviewer notes into **Notes for certification**.

## Firefox Add-ons (AMO)

Where: https://addons.mozilla.org/developers/ (free; sign in with a Mozilla account).

- **Submit a new add-on** › **On this site** (listed on addons.mozilla.org).
- **Platforms:** Firefox for desktop only (not Android: the Marrow app it talks to runs on computers).
- Upload `_firefox.zip`. Mozilla's automatic check should show 0 errors. One warning about Firefox for Android is expected, because the add-on isn't for Android.
- **Source code:** "No". The add-on isn't minified, bundled or generated.
- **Categories:** Other (and Bookmarks if a second is allowed).
- **Data collection:** the manifest declares `"data_collection_permissions": { "required": ["none"] }`: nothing leaves the user's computer.
- **License:** MIT.
- **Privacy policy:** paste the privacy policy text or link it.
- Paste the reviewer notes into **Notes to reviewer**.
- The add-on's ID is `marrow-clipper@cicadama.com`. Keep it; every update must use the same ID.

## After the first approval

- Updates: upload the new version's package in each store's dashboard. The version number must go up each time (it follows the app's version, set by `pack.py`).
- Once the store pages exist, add their links to `clipper/README.md` and to the Download section of https://www.cicadama.com/marrow/.
