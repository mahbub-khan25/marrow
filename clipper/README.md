# Marrow Clipper

A browser extension that saves the page you're reading, or the part you
selected, as a note in Marrow. Works in Chrome, Edge, Brave and Firefox.

The article is picked out of the page (menus, ads and footers are left
behind), turned into Markdown, and saved in your vault's **Clippings**
folder, with the page's address on top:

```markdown
---
source: https://example.com/post
clipped: 2026-10-30
---

# The post's title

The article…
```

## Using it

Marrow must be open on the same computer (it can keep running in the tray:
Settings › Editor › "Keep running for reminders").

- Click the Marrow button in the toolbar (or press **Alt+Shift+M**), then
  **Clip this page** or **Clip the selection**.
- Or right-click a page or a selection: **Clip to Marrow**.

## Installing

Until it's in the browsers' stores, install it from the release:

1. Download `Marrow_Clipper_<version>.zip` from the
   [latest release](https://github.com/mahbub-khan25/marrow/releases/latest)
   and unzip it into a folder you'll keep.
2. **Chrome, Edge, Brave:** open `chrome://extensions` (`edge://extensions`),
   turn on **Developer mode**, click **Load unpacked**, and pick the folder.
3. **Firefox:** open `about:debugging#/runtime/this-firefox`, click **Load
   Temporary Add-on…**, and pick `manifest.json` in the folder (Firefox
   forgets it when it closes, until the add-on is signed by Mozilla).

## Privacy

The extension reads a page only when you clip it, and sends it only to
Marrow on your own computer (`127.0.0.1`). Nothing goes anywhere else.
Marrow answers only browser extensions, so web sites can't use it.
