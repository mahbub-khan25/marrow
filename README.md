# Marrow

A local-first Markdown notes app in Rust. Your notes are plain `.md` files in
a folder; Marrow indexes them, resolves `[[wikilinks]]` and shows backlinks.

**[Download the latest beta](https://github.com/mahbub-khan25/marrow/releases)**
for Windows, macOS and Linux. Marrow is free and open source under the
[MIT license](LICENSE). Bug reports and pull requests are welcome; see
[CONTRIBUTING.md](CONTRIBUTING.md).

**Status:** Phases 0–8 of [PLAN.md](PLAN.md) and the redesign work: open a
folder, read notes rendered or write them as Markdown, link notes and see
backlinks, rename or move notes without breaking links, explore a graph, lay
notes out on a canvas, search the whole vault, browse tags, keep a daily note,
work in tabs and a split pane that reopen where you left off, and never lose
text to sync apps or failed saves.

## Writing extras

* **Format visually or by hand**: the toolbar above the editor (and the small
  bar that appears over selected text) sets headings, bold, italic,
  strikethrough, highlight, code, links, lists, to-dos, quotes, tables and
  dividers. Ctrl+B, Ctrl+I, Ctrl+Shift+X (strike) and Ctrl+Shift+H
  (highlight) work too. It all writes Markdown, so typing `#`, `**`, `-` by
  hand, or HTML like `<h1>`, works just as well. Settings → Editor hides the
  toolbar.
* **Diagram editor**: hover a Mermaid diagram and click *Edit diagram* (or
  double-click it), or type `/diagram` for a new one. Code sits on one side
  and a live preview on the other, with templates for flowcharts, sequence,
  mind maps, timelines, Gantt, pie, state, class and ER diagrams.
  Flowcharts also have a **Visual** mode:
  * click a box to rename it or change its shape;
  * add boxes or connected boxes;
  * draw and label arrows;
  * change the direction.

  It's saved as ordinary Mermaid text. Ctrl+Enter saves.
* **Math, footnotes, diagrams, code**: `$x^2$` and `$$ … $$` render with
  KaTeX; `[^1]` footnotes; ` ```mermaid ` diagrams; code blocks are coloured
  by language.

All of these are plain Markdown, so the notes still open in Obsidian.

* **Type `/`** at the start of a line (or after a space) for headings, lists,
  to-dos, toggles, callouts, tables, code, math, links, the date and your
  templates. Keep typing to filter; Enter inserts.
* **Collapse** a section or a list: hover a heading or list item and click the
  arrow beside it. In Source the arrows sit in the left margin. Marrow
  remembers what you folded.
* **Callouts** are coloured by type. Click a callout's icon in Preview to
  change its type.
* **Icon and cover**: hover above a note's title for *Add icon* and
  *Add cover*. They're stored in the note's properties (`icon:`, `cover:`).
* **Templates**: notes in the `Templates` folder (change it in Settings → Vault)
  appear in the `/` menu. `{{title}}`, `{{date}}` and `{{time}}` are filled in.
* **Properties**: a note's frontmatter shows under its title in Preview as
  fields you can edit: text, numbers, checkboxes, dates, lists, tags and links
  to other notes. Click a property's name to change its type, rename or
  remove it; *Add property* adds one.
* **Move blocks**: hover a paragraph, heading, list item, table or callout in
  Preview and drag the handle that appears in the margin. Click the handle to
  duplicate, move or delete the block.
* **Tables**: click a cell in Preview to edit it. Tab moves on (and adds a row
  at the end); right-click for rows, columns, sorting and alignment; the +
  bars add a row or a column.
* **Columns**: `/2 columns` or `/3 columns`. It's a ` ```columns ` block with
  the parts separated by a `+++` line (Obsidian shows it as a code block).
* **Databases**: `/database`, or right-click a folder → *View as database*.
  It shows the notes in a folder as a **table**, **board**, **calendar** or
  **gallery**, using their properties. Switch views with the tabs. In each view
  you can:
  * click a cell to edit it;
  * drag cards between board columns or calendar days;
  * add a note with *New*;
  * click a column header to sort.

  The block is plain YAML:

  ```
  ```database
  from: Projects/Tasks      # default: this note's folder
  view: board               # table, board, calendar or gallery
  columns: [status, due, project, project.owner]
  group: status             # board columns
  date: due                 # calendar
  sort: due desc
  filter: status != Done and due < today
  ```
  ```

  **Relations and rollups**: a property holding `[[links]]` is a relation, and
  `project.owner` reads `owner` from the linked note. `count()`, `sum()`,
  `avg()`, `min()`, `max()` and `percent()` roll values up, and `backlinks`
  means the notes linking here. For example:
  `columns: [Tasks = count(backlinks), Hours = sum(backlinks.hours), Done = percent(backlinks.done)]`
* **Tasks**: the ✓ button on the left lists every `- [ ]` to-do in the
  vault: *Overdue*, *Today*, *Upcoming*, unticked ones left in earlier daily
  notes, and those with no date. Tick one to tick it in its note; click it
  to open the note there. Give a task a date with `📅 2026-10-20`,
  `due: 2026-10-20` or a `[[2026-10-20]]` link. The box at the top adds a
  task to today's note.
* **Quick capture**: press **Ctrl+Alt+N** in any app while Marrow is open
  (even minimised) to jot something into today's note — a timed line under
  *Log*, or a to-do under *Tasks*. Add a voice note or a photo too; with an
  OpenAI or Groq key in Settings → AI, voice notes are also written out as
  text. On phones and tablets, tap the round **+** button. Change or turn
  off the shortcut in Settings → Editor.
* **Time machine**: right-click a note → *Time machine…*, or *Time
  machine* in the command palette for the whole vault. Drag back to any
  date to see notes as they were, what changed since, and bring back
  deleted notes.
* **Notes come back to you**: the start screen shows a forgotten note and
  the same day in earlier years, and a weekly review from Friday to Sunday.
* **Bangla**: a Bangla interface (Settings → Appearance), Bangla typing
  anywhere with [Ekushe Next](https://github.com/mahbub-khan25/ekushenext)
  (F12), and search that forgives Bangla spelling.
* **Move from Notion or Evernote**: *Move from Notion (step by step)* in the
  command palette, or on the start screen of a new vault.
* **Version history**: right-click a note → *Version history…* (or the
  palette). While you edit, Marrow keeps a copy every few minutes in
  `.marrow/history`; older copies thin out to one an hour, then one a day,
  for a year. Pick a time to see what changed since, then *Restore this
  version* — the text you had is kept too, so a restore can be undone.
* **Starred notes**: right-click a note → *Star* (or *Star / unstar this
  note* in the palette). Starred notes sit at the top of the file tree, and
  stay starred when renamed or moved.
* **Unlinked mentions**: the inspector lists notes that name this one (or one
  of its `aliases`) in plain text. *Link* turns that word into a link.
* **Find and replace**: search for a word or phrase, then click *Replace…*
  under the search box (or *Find and replace in all notes* in the palette).
  *Replace all* asks first and shows how many places will change.
* **Focus mode** (F11): hides everything but the note. Settings → Editor →
  *Word goal* adds a bar that fills as the note grows.
* **Images and files**: paste a screenshot (Ctrl+V) or drag pictures and
  files from your computer into a note. They're saved in the vault's
  `attachments` folder and linked where you put them — images as
  `![[Pasted image 20261015123500.png]]`, other files as `[[report.pdf]]`.
  In Preview, a dropped file goes after the paragraph you drop it on.
* **Link previews**: rest the pointer on a link in Preview (or hold Ctrl and
  point at one in Source) to read that note — or the linked section or
  image — in a small card. *Open* opens it.
* **Export and print**: right-click a note in the file tree, or use the
  command palette: *Export to PDF…*, *Export to HTML…*, *Print…*.
  * PDF goes through the print window: choose **Save as PDF** (or Microsoft
    Print to PDF) as the printer.
  * HTML is one file with the images, diagrams, maths and fonts inside, so
    it opens anywhere, even offline.
  * Both look like Preview in the light theme, with folded sections opened
    and editing buttons left out.
* **Synced blocks**: click a block's handle → *Copy as synced block*, then
  paste into any note. It's an Obsidian block embed (`![[Note#^id]]`). Hover
  the embed and click *Edit* to change it in place; the original note changes,
  and so does every other place that embeds it. Embedded sections
  (`![[Note#Heading]]`) and whole notes (`![[Note]]`) can be edited the same way.
* **Buttons**: a ` ```button ` block becomes a button in Preview.
  `action: append` adds a template below the button, `action: new-note`
  makes a new note from it (in `folder:` if given), and `action: daily`
  adds it to today's note.

## Locked notes

Right-click a note → **Lock with password…**, or a folder → **Lock folder
with password…** (also in the command palette).

* On disk the note becomes unreadable text. Only someone with the password can
  read it, in Marrow or anywhere else. Its file name stays visible.
* Opening it asks for the password once. It then works like any other note
  until you choose **Lock all notes now** (Ctrl+K), Marrow has been left
  alone for the time set in Settings → Editor → *Locked notes* (15 minutes by
  default), or you close Marrow.
* In a locked folder every note shares the folder's password, and new notes
  there are locked as soon as they're saved.
* A locked note's text is left out of search, tasks, the graph, backlinks
  and AI search, and its earlier versions are kept locked too. Attachments
  (images, PDFs) aren't locked.
* **Remove lock…** turns it back into plain Markdown.
* There's no way to recover a forgotten password. The optional hint is shown
  when Marrow asks for it.

The encryption is Argon2id (64 MiB) for the password and XChaCha20-Poly1305
for the text. Obsidian shows a locked note as a code block of letters.

## Import from other apps

*Import notes…* (command palette, Settings → Vault, or right-click empty
space in the file tree) brings notes in from:

* **Notion** — in Notion, Export → “Markdown & CSV” with subpages, and pick
  the .zip. Page names lose Notion's long ids, links between pages become
  `[[links]]`, page properties become note properties, pictures come along,
  and each database becomes a note with a database view of its pages.
* **Evernote** — export notebooks as .enex (File → Export Notes). Tags,
  dates, the source address, pictures, attachments and checkboxes come
  across; each .enex becomes a folder.
* **OneNote and Word** — in OneNote, File → Export → Word Document; any
  .docx works. Headings, bold/italic, links, lists, tables and pictures
  come across.
* **Web pages and Markdown** — saved .html pages (with their pictures) and
  .md or .txt files from other apps.

Everything goes into a new folder (“Notion”, “Evernote”…); the files you
pick are only read.

## Daily notes and the calendar

Daily notes are kept by year and month, so years of them stay easy to find:
`Daily/2026/10 October/2026-10-24.md`. The file name is always the date, so
`[[2026-10-24]]` links never break and other apps sort them correctly.

* **Calendar** (rail): a month of days with a dot where there's a note and a
  pink dot where tasks are due. Click a day to open its note, or start one.
  Click the month name to see the whole year at a glance.
* **On this day**: notes from the same date in earlier years.
* **Older notes** kept in one folder still work everywhere. The calendar and
  Settings → Vault offer to move them into year and month folders in one
  click; links keep working. Prefer one folder? Pick *One folder* in
  Settings → Vault.

## Writing comfort

* **Spell check**: misspelt words are underlined in Source; right-click one
  for suggestions. It uses your computer's own dictionaries and languages
  (on Windows: Settings → Time & language → Language). Turn it off in
  Settings → Editor.
* **Presentation mode**: Ctrl+K → *Present this note as slides*. Put a line
  with just `---` between slides (as in Obsidian), or let Marrow start a
  new slide at each heading. → / Space for the next slide, ← to go back,
  Esc to stop. It goes full screen; long slides shrink to fit.
* **Vim keys**: Settings → Editor → *Vim keys*. Normal, insert and visual
  modes, `:w` saves.

## Customize: CSS snippets and plugins

Settings → **Customize**.

* **CSS snippets** restyle Marrow. They're `.css` files in the vault's
  `.marrow/snippets` folder (so they travel with the vault); switch each one
  on or off. *Example snippet* writes one to start from; *Reload* picks up
  your edits.
* **Plugins** add commands to the command palette. Each is a folder in
  `.marrow/plugins` with a `manifest.json` and a `main.js`. *Example plugin*
  adds "Writing tools" (insert the date, sort lines, Title Case, count
  words) to copy from; the help under the list shows the whole API.
  * A plugin runs in a sandbox: it can't reach the internet, Marrow's own
    window or your files. It can only ask Marrow to do things through a small
    API, and only what its manifest asks for and you allow when you turn it
    on: `read` (read notes) and `write` (change and create notes).
  * Locked notes are never shown to plugins.

## Publish as a website

Right-click a folder (or the empty space in the file tree, for the whole
vault) → **Publish as a website…**, or use the command palette.

* Pick a site title and an empty folder outside the vault. Marrow writes
  one page per note, with working links, images, maths, diagrams, code
  colours, callouts and embedded sections, plus a sidebar of pages,
  "Linked from" lists, a search box (press `/`) and a phone-friendly menu.
* A note called *Index*, *Home*, *README* or *Welcome* at the top becomes
  the home page; otherwise the home page lists every page.
* Left out: locked notes, notes with `publish: false` in their properties,
  and — if you tick *Only notes marked publish: true* — every note without
  that property. Links to notes that aren't published stay as plain words.
* Publish again to update the site; pages for notes you removed are deleted
  from it. Marrow only ever removes files it wrote.
* Nothing is uploaded. To put it online, drag the folder onto Cloudflare
  Pages or Netlify ("deploy manually"), or push it to a GitHub Pages repo.
  Search needs the site to be served from a web address; everything else
  also works when you open `index.html` straight from the folder.

## AI (bring your own)

Marrow has no AI of its own and sends nothing anywhere until you connect one in
**Settings → AI**. Use your own account with any of these, or a free model
running on your computer:

* **Popular**: OpenAI (ChatGPT), Anthropic Claude, Google Gemini, Mistral,
  xAI Grok, Groq, OpenRouter, Perplexity.
* **Chinese**: DeepSeek, Qwen (Alibaba Cloud), Kimi (Moonshot AI),
  GLM (Zhipu AI / Z.ai), ERNIE (Baidu Qianfan), Doubao (ByteDance Volcano
  Engine), MiniMax, Hunyuan (Tencent).
* **On this computer**: Ollama and LM Studio — free, private, nothing leaves
  the computer.
* **Other**: any service that works like the OpenAI or Anthropic API
  (paste its address).

Pick the provider, paste the key, click **Load models** and pick one, then
**Test** and **Save**. You can add several and choose which one to use each
time. Keys are kept in Windows Credential Manager (macOS: Keychain); in the
portable version and on Linux they're kept in a file next to Marrow's
settings — never in your notes.

Using it:

* **Select text** (in Source) and click **AI** on the small bar, or press
  **Ctrl+J**: improve writing, fix spelling & grammar, shorter, longer,
  simplify, change tone, translate, summarize, explain, turn into a list or
  table, continue writing — or type what you want. The answer streams in;
  then **Replace**, **Insert below**, **Copy**, try again, or tell it what to
  change.
* **`/ai`** on an empty line writes at the cursor: continue writing,
  brainstorm, outline, action items, a table, or anything you type.
* **Whole note** (works in Preview too): **Ctrl+J** in Preview, or
  *AI: Ask about this note* in the command palette — summarize, key points,
  action items, title and tag ideas, gaps, translate, or ask a question. The
  answer can be added to the end of the note.

* **Ask my notes** (**Ctrl+Shift+J**, or *AI: Ask my notes* in the palette):
  ask a question of the whole vault. Marrow finds the parts of your notes
  that match, sends only those to the AI, and the answer cites them — click
  a number to open that note there. *Save as a note* keeps the answer with
  links to its sources.
* **Search by meaning**: Settings → AI → *Search by meaning* — pick an AI
  with an embedding model (Ollama's `nomic-embed-text` is free and stays on
  your computer; OpenAI, Gemini, Mistral, Qwen and others work too) and
  click *Build the index*. Then search has a *Words / Meaning* switch, and
  Ask my notes finds notes by what they mean, not only their words. The
  index lives in Marrow's cache, not in your vault, and new notes are added
  as you go.
* **AI columns**: in a database's table, *AI column* asks the AI to write a
  value for each note — a one-line summary, a category, the next step, or
  your own instruction — saved as a property of each note.

Only the open note is sent (or, for Ask my notes, the matching parts of
your notes), and only when you ask.

## Cloud drives

On a computer, Marrow can use the folders that the Dropbox, OneDrive, Google
Drive and iCloud apps keep on your disk. On any device, including phones and
tablets, it can also sync directly with your own server.

* **Create a vault in the cloud**: *Create a vault* (on the start screen, in
  Settings → Vault, or in the command palette) lists the cloud folders it
  finds. Pick one, and the cloud app carries the vault to your other devices.
* **Sync a local vault with the cloud**: Settings → Vault → Sync →
  *Set up…*. Marrow keeps the vault and a cloud folder the same, both ways,
  every few seconds while it's open. A cloud icon in the status bar shows the
  state; click it to sync now.
  * A note edited on both sides is merged, so nothing you wrote is lost.
    Other files changed on both sides keep both versions.
  * Files deleted on one side are deleted on the other. On this computer
    they go to `.marrow/trash`; the cloud app keeps its own deleted files.
  * If the cloud folder goes missing or suddenly empty, or a sync would
    delete many files at once, sync pauses and asks first.
  * Hidden files and folders (`.marrow`, `.obsidian`, `.git`) aren't synced.
* **Sync with your own server (WebDAV)**: Settings → Vault → Sync →
  *Set up…* → *Nextcloud, Synology or another WebDAV server*. Enter the
  folder's WebDAV address, your user name and password (for Nextcloud, an app
  password from Settings › Security). Marrow checks the sign-in first. Notes
  stay plain Markdown files in that folder; the password is kept in the
  system's credential store. Works with Nextcloud, ownCloud, Synology, pCloud,
  Koofr and other WebDAV servers, the same way as the cloud folders above.
  Direct Google Drive, OneDrive, Dropbox and iCloud sync are coming next.
* **End-to-end encryption** (optional, in the same setup): tick *Encrypt
  everything before it leaves this device* and choose a sync passphrase.
  Every file and every file name is encrypted on your device (Argon2id and
  XChaCha20-Poly1305, as for locked notes), so the cloud or server only holds
  scrambled data. Enter the same passphrase on your other devices. Marrow
  can't recover a lost passphrase: the copy in the cloud then can't be read,
  but the notes on your devices are fine. Encrypted sync needs an empty
  folder, so plain and encrypted files never mix.
* **Conflict copies**: when a cloud app keeps two versions of a note (for
  example `Note (Khan's conflicted copy …).md`), Marrow offers to merge them
  back into one.

## Portable Windows version (no install)

`npm run portable` builds `portable\Marrow\` and `portable\Marrow-portable.zip`.
Copy the folder anywhere (a USB stick, Documents, OneDrive) and run
`Marrow.exe`. It needs no installer and no admin rights.

* `portable.txt` next to `Marrow.exe` turns on portable mode. Settings, the
  search cache, backups and window state (tabs, preferences) are then kept in
  `Data\` beside it, and nothing is written to the PC's user folders.
* The `Notes` folder next to it opens automatically the first time. Vaults
  inside the Marrow folder are remembered relative to it, so a USB stick works
  whatever drive letter it gets.
* It carries its own **WebView2** (the part of Windows that draws the window)
  in `WebView2\` and always uses that copy, so it doesn't matter what the
  other PC has. By default the build copies the runtime installed on your PC;
  `-WebView2Cab <file>` uses Microsoft's "Fixed Version" download instead,
  and `-NoWebView2` leaves it out for a much smaller folder. On Windows 10,
  Marrow grants the permissions this needs by itself on first run (Microsoft's
  documented `icacls` step; no admin needed).
* It can't run from a network share (`\\server\...`); WebView2 doesn't
  support that. Local folders, OneDrive folders and USB sticks are fine.

## Search syntax

| Type | Finds notes that… |
|---|---|
| `rust cache` | contain both words, anywhere, any case |
| `"link resolver"` | contain the exact phrase |
| `-draft` | don't contain the word |
| `tag:#idea` or `#idea` | have the tag or a sub-tag like `#idea/later` |
| `path:Projects` | are in a folder whose path contains the text |
| `file:2026` | have the text in their name |

## Your text is safe

* **Another app edits the note you're typing in** (OneDrive, Dropbox, a phone):
  Marrow merges both versions. If you both changed the same lines, both are kept.
* **A save fails** (file locked, disk full): your text is kept in a backup and
  offered back the next time you open the note.

## Build from source

* All systems: Rust (stable, <https://rustup.rs>) and Node 20+.
* Windows: Visual Studio C++ Build Tools ("Desktop development with C++").
  WebView2 already ships with Windows 10/11.
* macOS: Xcode Command Line Tools (`xcode-select --install`).
* Linux: the Tauri system libraries, for example on Ubuntu
  `sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev`
  (see <https://tauri.app/start/prerequisites/>).

## Run the app

```powershell
git clone https://github.com/mahbub-khan25/marrow.git
cd marrow
npm install
npm run tauri dev
```

The first build compiles Tauri and takes a few minutes; later starts are quick.
Pick a folder of notes (an empty folder, or a copy of an Obsidian vault to be
safe while Marrow is young).

### Make a Windows installer

```powershell
npm run tauri build
```

This builds an optimised app and two installers:

* `target\release\bundle\nsis\Marrow_1.1.0_x64-setup.exe`, the usual choice. It installs for the
  current user, with no admin prompt, and adds Start menu and uninstall entries.
* `target\release\bundle\msi\Marrow_1.1.0_x64_en-US.msi`, for managed installs.

The installers download Microsoft's WebView2 runtime only if the PC doesn't already have it
(Windows 10 and 11 normally do). The version comes from `src-tauri\tauri.conf.json`
and shows in Settings → About.

| Action | How |
|---|---|
| Command palette: notes, text, `#tags`, commands | Ctrl+K (also Ctrl+O, Ctrl+P); Ctrl+T opens the pick in a new tab |
| New note | Ctrl+N, the pencil above the file tree, or the + in the tab bar |
| Today's daily note | Ctrl+D, or the calendar-page icon in the left rail |
| Calendar | the calendar icon with dots in the rail: click a day to open or start its note; click the month name for the whole year; *On this day* lists the same date in earlier years. Dots mark days with a note (blue) and tasks due (pink) |
| Preview / Source | Ctrl+E, or the switch above the title; double-click a paragraph to edit it |
| Search all notes | Ctrl+Shift+F (select a word first to search for it) |
| Tags | the # in the left rail; click a `#tag` in a note |
| Split pane | `Ctrl+\`; Ctrl+Enter in the palette or Ctrl-click a link in preview opens it there |
| Inspector (local graph, outline, mentions, links) | the right-most button in the tab bar |
| Graph | Ctrl+G, or the graph icon in the rail. Click a dot to inspect it, double-click to open; `/` filters; Forces adjusts the layout |
| Canvas | the squares icon in the rail; right-click a folder → New canvas here. The toolbar at the bottom draws boxes, ellipses, diamonds, arrows and pen lines in six colours (keys V T R O D A P); double-click a shape or an arrow to label it; Ctrl+Z undoes |
| Open in a new tab | Ctrl-click or middle-click in the file tree or search results |
| Switch / close tabs | Ctrl+Tab, Ctrl+Shift+Tab (or Ctrl+PageDown / PageUp); Ctrl+W |
| Follow a link (Source) | Ctrl-click it, or put the cursor in it and press Ctrl+Enter |
| Link a note | type `[[` and pick from the list |
| Save now | Ctrl+S (it also saves 2 s after you stop typing) |
| Search in the note | Ctrl+F (Source) |
| Rename a note | click its title and type, or F2 in the file tree (links to it are updated) |
| Move a note or folder | drag it onto a folder in the file tree |
| Delete | right-click in the tree, or select and press Delete (goes to `.marrow\trash`) |
| Settings (theme, text size, line length, daily folder, link format) | Ctrl+, or the gear in the rail |
| Switch vault | click the vault name at the top left, or Settings → Vault |

## Engine only

```powershell
cd marrow
cargo test

# Point the engine at any folder of notes (an Obsidian vault works; it only reads it):
cargo run --release --example inspect -- "D:\path\to\vault"

# Show one note's links and backlinks:
cargo run --release --example inspect -- "D:\path\to\vault" "Folder/Note.md"

# Watch live: edit notes in another app and see the index react:
cargo run --release --example inspect -- "D:\path\to\vault" --watch
```

Run `inspect` twice on the same vault. The second run should report every
note as *unchanged (not read)*. That is the cache working.

On a generated vault of 5,000 notes: first open about 0.4 s, second open about
0.15 s (Linux; Windows file-system calls are slower, so expect some multiple
of that).

## Layout

```
Cargo.toml                 workspace (marrow-core + src-tauri)
package.json, vite.config  frontend build (source in ui/)
src-tauri/                 Tauri shell: commands.rs bridges the window to the engine
ui/src/main.ts             app shell: tabs, split, opening, rename/move, live updates
ui/src/docs.ts             open notes: saving, merging outside edits, backups
ui/src/notepane.ts         one note: header, title rename, preview / source
ui/src/preview.ts          Markdown → HTML (wikilinks, embeds, tags, tasks, callouts)
ui/src/editor.ts           CodeMirror: wikilinks, Ctrl-click, [[ autocomplete
ui/src/tree.ts             file tree (grouped folders, drag and drop, context menu)
ui/src/sidebar.ts          Search and Tags panes
ui/src/inspector.ts        local graph, outline, linked mentions, outgoing links
ui/src/palette.ts          Ctrl+K command palette
ui/src/settings.ts         settings dialog
ui/src/canvas.ts           canvas boards (.canvas, JSON Canvas)
ui/src/graph.ts            graph renderer (canvas + d3-force, animation)
ui/src/graphpanel.ts       graph controls: legend, filters, forces, minimap, node card
ui/src/style.css           design tokens, components, layout; dark theme
crates/marrow-core/
  src/path.rs              canonical vault-relative paths (NFC, `/`, no escapes)
  src/vault.rs             file tree, reconcile against disk, write/rename/trash
  src/parse.rs             Markdown → links, embeds, tags, headings, ^blocks, tasks, YAML
  src/cache.rs             redb cache: path → (mtime, size, hash), hash → metadata
  src/links.rs             filename index, link resolution, link graph, backlinks
  src/engine.rs            one owner for all of the above
  src/engine/rename.rs     rename/move with link rewriting
  src/engine/graph.rs      nodes and edges for the graph view
  src/engine/search.rs     full-text search and query parsing
  src/merge.rs             three-way merge for outside edits
  src/backup.rs            text kept when a save fails
  src/service.rs           engine + debounced file watcher thread
  examples/inspect.rs      command-line inspector
```

## License

MIT: see [LICENSE](LICENSE). Bundled fonts and libraries keep their own
licenses (listed in [THIRD_PARTY.md](THIRD_PARTY.md)). Marrow is not affiliated with Obsidian and contains none
of its code; behaviour was described in a separate design doc and
re-implemented from that description.
