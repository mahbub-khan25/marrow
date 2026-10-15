# Marrow — build plan

A local-first Markdown notes app in Rust (Tauri + CodeMirror 6), designed
from a clean-room study of how Obsidian behaves. All file, index and link
logic lives in `crates/marrow-core`, which has no UI code and is tested
on its own. The Tauri app is a thin shell around it.

## Phases

| # | Phase | Status | Done when |
|---|-------|--------|-----------|
| 0 | Scaffold: workspace, `marrow-core`, tests | ✅ done | `cargo test` passes |
| 1 | Vault model: scan, in-memory tree, path normalisation, reconcile, file ops, local trash | ✅ done | 7 vault + 5 path tests |
| 2 | Parser + content-hash cache (`redb`) | ✅ done | Second open of a vault parses 0 notes |
| 3 | Filename index, link resolution, link graph, backlinks, link text | ✅ done | Every resolution rule has a test |
| 4 | Watcher (`notify` + debounce) → reconcile → incremental re-index | ✅ done | External create/edit/delete updates backlinks in tests |
| 5 | Tauri MVP: open vault, file tree, CodeMirror editor, 2 s debounced save, wikilink click + autocomplete, backlinks panel | ✅ done | Tested in the real app: type → saved to disk; Ctrl-click creates missing notes; outside edits appear live |
| 6 | Rename/move from UI with link rewriting (snapshot → rename → offset splices) | ✅ done | Renaming fixes every `[[link]]` |
| 7 | Three-way merge of outside edits into a dirty editor; backup on save failure | ✅ done | No lost text when another app edits an open note |
| 8 | Full-text search, quick switcher, tags pane, tabs + session restore | ✅ done | Ctrl+O and Ctrl+Shift+F work |
| R | Redesign to the Claude Design prototype: new shell, preview mode, inspector, palette, settings, daily notes, split pane, canvas | ✅ done | Screens match the prototype side by side |
| 9 | Live preview in Source, Windows installer, version in settings, local graph depth | ✅ done | Installable build |

## Architecture

```
disk ──watch──▶ VaultService (thread) ──lock──▶ Engine ◀──lock── Tauri commands
                                                 │
                    ┌────────────┬───────────────┼──────────────┐
                  Vault        Cache          parse()       LinkGraph
             (path → stat)  (redb: path→hash,  (pulldown-   (filename index,
                             hash→NoteMeta)     cmark + own   resolved /
                                                scanner)      unresolved maps)
```

* **One owner.** All state changes go through `&mut Engine` behind one mutex,
  so a save and a watcher event can never interleave.
* **Reconcile, don't trust.** Watcher events only say *which path*; the vault
  stats it and compares mtime + size to decide created / modified / deleted.
  Own writes reconcile immediately, so the watcher's echo finds nothing new.
* **Content-addressed cache.** `files: path → (mtime, size, blake3)` and
  `meta: hash → NoteMeta`. Unchanged files are not read; copies and reverts are
  not re-parsed. The cache lives in the app data folder, never in the vault.
* **Backlinks on demand** from the resolved-link map, then re-resolved per
  reference for exact positions. No reverse index to keep consistent.
* **Hidden folders are never indexed.** Settings go in `<vault>/.marrow/`;
  deleted files go to `<vault>/.marrow/trash/`.

## Differences from Obsidian (deliberate)

* Partial-path link matching respects segment boundaries: `[[x/b]]` matches
  `y/x/b.md` but not `ax/b.md`.
* Delete defaults to the vault-local trash.
* Notes are parsed in parallel (`rayon`) instead of one worker thread.

## Phase 5 — what shipped

- [x] `src-tauri/`: Tauri 2 app holding a `VaultService` in managed state
- [x] Commands: `open_vault`, `last_vault`, `tree`, `read_note`, `write_note`, `create_note`,
      `create_folder`, `trash`, `rename`, `backlinks`, `note_links`, `resolve_link`,
      `follow_link`, `find_files`
- [x] Engine events (watcher *and* the app's own changes) forwarded as `vault-changed`
- [x] `ui/`: Vite + TypeScript, CodeMirror 6 with Markdown + code-block languages
- [x] File tree (folders remember open/closed), context menu, Delete key → vault trash
- [x] Editor: serif writing column, wikilinks styled resolved / unresolved
- [x] Save 2 s after typing stops, on window blur, on note switch, before close; Ctrl+S
- [x] Ctrl-click or Ctrl+Enter on `[[link]]` opens it, creating the note if missing
- [x] `[[` autocomplete using the engine's link-text rules
- [x] Backlinks panel with line snippets (click jumps to the line) + "Not created yet"
- [x] Quick switcher (Ctrl+O / Ctrl+P), create from it with Shift+Enter
- [x] Outside edit to the open note: reloads if clean, warns if you have unsaved edits
- [x] Remembers the last vault (`settings.json` in the app config folder)

## Phase 6 — what shipped

- [x] Rename by clicking the note title, or F2 / right-click → Rename in the tree (notes and folders)
- [x] Move by dragging a note or folder onto a folder (or the empty tree area for the vault root)
- [x] Before renaming: snapshot every reference with the file it resolves to now
- [x] After renaming: new link text only where needed (a bare `[[Name]]` that still works is left alone),
      keeping `#heading`, `^block`, aliases, embeds and `\|` inside tables
- [x] Markdown links `[x](Note.md)` and frontmatter `"[[Note]]"` values are rewritten too
- [x] Name checks: empty, leading dot, `\ / : * ? " < > |`, and clashes in the same folder
- [x] Notice: "Renamed. Updated 3 links in 3 notes."
- [x] 7 engine tests (all link kinds, folder move, name clash, relative links, code blocks, self-links, case-only)

## Graph view (from Phase 9, done early)

- [x] Ctrl+G or the graph icon; Esc returns to the note
- [x] Whole vault, or "This note" with 1–3 steps away
- [x] Optional dots for missing notes (dashed) and attachments
- [x] Canvas + d3-force: scroll to zoom, drag to pan, drag dots, hover highlights neighbours, click opens
- [x] Fits the view once the layout settles; labels for hubs when zoomed out, all labels when zoomed in
- [x] Live: updates as notes and links change

## Phase 7 — what shipped

- [x] `merge3(base, mine, theirs)`: line-based three-way merge that never drops text and never
      writes conflict markers. Where both sides changed the same lines, both versions are kept, yours first.
- [x] Outside edit to a note with unsaved changes: merged into the editor with one minimal change,
      so the cursor stays put; the result is saved; a notice (clean) or banner (overlap) says what happened
- [x] Guarded saves: every save says which version it expects on disk. If another app wrote in the
      moment before the watcher reported it, the save is refused, merged, then written. Closes the race.
- [x] Save failure: text goes to a backup in the app data folder (outside the vault), the banner says
      why in plain words ("The file is locked or read-only…") with **Try again**
- [x] Reopening a note with a backup offers **Restore it** / **Discard**; the backup is deleted after
      the next successful save, and follows the note through renames
- [x] 11 new tests (10 merge, 1 backup); tested in the real app: different-place edits, same-line
      edits, write-then-instant-save race, locked file + app restart + restore

## Phase 8 — what shipped

- [x] Full-text search (Ctrl+Shift+F, or select a word first to search for it). Design change from
      the plan: no `tantivy` index. Like Obsidian, Marrow scans note text kept in memory, so results are
      always exact and current with nothing to keep in sync. Text loads in the background when a vault
      opens; only notes whose size or modified time changed are re-read. 5,000 notes: ~60 ms for the
      first search, 6–10 ms after.
- [x] Query syntax: words (all must appear), `"exact phrase"`, `-exclude`, `tag:#idea` (or `#idea`,
      includes sub-tags), `path:folder`, `file:name`. Name matches rank first, then most matches.
- [x] Results with highlighted matches (UTF-16 safe, so emoji and accents line up); click jumps to the
      line and selects the match; Ctrl-click opens in a new tab
- [x] Tags pane: nested tags as a tree (`#a/b` counts towards `#a`), click to search; Ctrl-click a
      `#tag` in a note does the same
- [x] Tabs: each keeps its own undo history, cursor and scroll. Ctrl-click or middle-click in the tree,
      Ctrl+T (quick switcher into a new tab), Ctrl+W, Ctrl+Tab / Ctrl+Shift+Tab, Ctrl+PageDown / PageUp
- [x] Session restore per vault: open tabs, active tab, cursor and scroll (saved as you move, so it
      survives a crash)
- [x] Backlink and search jumps count columns in UTF-16 like the editor
- [x] 6 new search tests; tested in the real app (search, tags, tabs, restart restore, undo across tabs)

## Redesign — what shipped

Built from the Claude Design prototype (`Marrow.html`). Measurements and
colours come from the prototype's own source, not from eyeballing.

- [x] Look: Source Serif 4 throughout, Phosphor duotone icons, the "Broadsheet"
      tokens (paper, ink, cyan and magenta accents). Dark mode reverses the tonal
      ramps; System / Light / Dark in settings
- [x] Shell: icon rail (Files, Search, Tags · Today, Graph, Canvas, Palette,
      Settings), sidebar with vault name and "N notes · M links", tab bar with
      "Search or jump to…", split and inspector buttons, status bar with
      backlinks · words · characters and the save state
- [x] File tree grouped by top-level folders (small caps, counts), nested folders
      indent; canvases open from the tree; the daily folder sorts newest first
- [x] Notes open in **Preview** (rendered Markdown) by default; Ctrl+E or the
      Preview / Source switch flips; double-click a paragraph to edit it there
- [x] Preview renders wikilinks (unresolved in magenta), `![[image]]` and
      `![[note]]` / `![[note#heading]]` embeds, `#tags`, clickable task
      checkboxes, callouts, `==highlights==`, tables, code, external links
      (open in the browser)
- [x] Note header: folder, title (edit it to rename), tag chips, "Edited … ·
      N words · M backlinks"
- [x] Inspector: local graph (solid = links out, dashed = links in), outline
      (click to jump), linked mentions, outgoing links ("not written yet")
- [x] Command palette (Ctrl+K, also Ctrl+O / Ctrl+P): recent notes, note names,
      text inside notes with highlights, `#tag` filters, `>` for commands only,
      commands, "Create …". Enter opens, Ctrl+Enter opens in the split, Shift+Enter
      creates
- [x] Split pane (`Ctrl+\`): a second note beside the first; the same note can be
      open in both and edits stay in step. Ctrl-click a link in preview opens it there
- [x] Daily notes (Ctrl+D): `Daily/YYYY-MM-DD.md`, previous / next day arrows
- [x] Graph as a tab: Notes / Notes + tags, missing notes, attachments
- [x] Canvas (`.canvas`, JSON Canvas, opens in Obsidian too): note cards with
      excerpts, text cards, links, groups; drag, resize, connect (drag a card's
      dot onto another), pan, Ctrl+scroll zoom, fit, Delete; drop notes from the
      sidebar; double-click a note card to open it
- [x] Settings (Ctrl+,): Editor (open in, line length, text size), Appearance
      (theme, inspector, link style), Hotkeys, Vault (new note location, daily
      folder, link format, show in file manager, open another folder). Vault
      settings travel with the vault in `.marrow/app.json`
- [x] Start page with "Recently edited" when no tab is open; new app icon
- [x] Engine: `note_info`, `vault_stats`, `recent_files`, tag nodes in the graph
- [x] Saving moved into `ui/src/docs.ts` (one document per open note, shared by
      panes); merge, backup and the overwrite guard work as before (re-tested)

## Phase 9 — what shipped

- [x] Live preview in Source (`ui/src/livepreview.ts`, on by default, Settings → Editor):
      heading marks, `**` `*` `~~` `==` and inline backticks, `[[target|` `]]`, `[text](url)`
      and `> ` are hidden except on the lines the cursor is on; bullets become •, `---` a rule,
      `- [ ]` a checkbox you can click; frontmatter shows as quiet monospace; the cursor steps
      over hidden marks in one keypress
- [x] Ctrl-click follows `[text](url)` links in Source too (external ones open in the browser)
- [x] Settings → About with the app version
- [x] Local graph: 1 step / 2 steps (neighbours of neighbours on a fainter outer ring)
- [x] Installer metadata (publisher, description, per-user NSIS install, WebView2 bootstrapper);
      `npm run tauri build` gives a `-setup.exe` and an `.msi` on Windows. Release build checked on Linux

## Graph refresh — what shipped

- [x] Live, animated graph: dots drift gently, signals travel along each link in the direction it
      points, the selected dot breathes; panning to a dot or Fit glides there. "Motion" turns it off
      (off by default when the system asks for reduced motion)
- [x] Dots coloured by top-level folder or by each note's most common tag (print inks: cyan,
      magenta, yellow, then companions), with a legend that highlights one group on click
- [x] Paper-rimmed dots, labels on soft badges, arrowheads, a ruled grid that moves with the view,
      soft vignette
- [x] Click a dot for a card: folder, tags, excerpt, incoming / outgoing counts, linked notes (click
      to hop), Open note (Enter) and Open in split. Double-click opens straight away
- [x] Filter box (`/` or Ctrl+F; Enter jumps to the first match), depth around the open note
      (whole vault, 1–3 steps), toggles for tags, missing notes, files, orphans, arrows, motion
- [x] Forces drawer: centre pull, repulsion, link length, live; Reset
- [x] Minimap (click or drag to move the view) with zoom buttons and Fit
- [x] Renderer pauses when the graph tab isn't showing; with more than 500 links only the
      highlighted links carry signals

## Notion-style features, Phase A — what shipped

* `/` menu (slash.ts): basic blocks, callouts, inserts, and templates from the
  vault's templates folder. Matches label words and keywords.
* Folding: CodeMirror fold gutter in Source (headings, lists), carets on
  headings and nested list items in Preview; heading folds persist per vault.
* Callouts: ten coloured kinds plus Obsidian aliases; folded callouts render
  as `<details>`; the icon opens a type menu.
* Icon, cover and breadcrumb: frontmatter `icon`/`cover`, edited with the
  `yaml` package so comments and order are kept. Covers outside the vault are
  copied into `attachments/` (`import_file` command).
* Template buttons: ` ```button ` blocks with `append`, `new-note`, `daily`.

## Cloud vaults and sync — what shipped

* `cloud.rs` finds Dropbox (from its `info.json`), OneDrive (environment
  variables), Google Drive (`X:\My Drive`, mirrored `~/My Drive`), and iCloud
  folders on Windows, plus `~/Library/CloudStorage` on macOS.
* *Create a vault* makes a folder in one of them (or anywhere) and opens it.
* `mirror.rs`: two-way sync between the vault and another folder.
  * Change detection uses time and size, confirmed by hash.
  * Notes changed on both sides get a three-way merge, using the last synced
    text kept in `.marrow/mirror/base`. With no base, both versions are kept.
  * Other files changed on both sides keep both versions.
  * Local deletions go to the trash.
  * Sync stops if the other folder is missing or emptied, or would delete
    more than 20 files and more than half of them.
* The sync worker thread (src-tauri `sync.rs`) runs every 15 s and shortly
  after each save. The setting is per computer, in the app settings.
* `conflicts.rs` recognises Dropbox, OneDrive, Google Drive, Syncthing and
  Marrow conflict copies of notes. The banner offers Merge (two-way union
  merge, then the copy goes to the trash), Compare, or Keep both.

## Notion-style features, Phase B — what shipped

* Property header (properties.ts). Shown in Preview only; types are inferred
  from values. Uses `yaml` edits via setProp/renameProp.
* Drag handles (blocks.ts). Blocks are top-level preview elements plus the
  items of top-level lists, mapped to line ranges through `data-line`.
  Moving rewrites lines and keeps blank-line spacing; a collapsed heading
  moves with its section.
* Table editor (tables.ts). Parses the GFM table at the clicked table's line,
  edits cells as raw Markdown, and writes the table back padded and aligned
  (wide characters count double).
* Columns: a ` ```columns ` fence split on `+++`. Each part renders with its
  own line offset, so double-click-to-edit still lands on the right line.

## Notion-style features, Phase C — what shipped

* Engine `query(folder, recursive)`. It returns a `NoteRow` for each note:
  * frontmatter as JSON;
  * resolved `[[links]]` per property (`prop_links`);
  * backlinks;
  * created and modified times.

  It also returns `related` rows for every note those point to, so relations
  and rollups resolve one hop away. Exposed as the `query_notes` command.
* `database.ts`: the ` ```database ` block (YAML: from, view, columns, group,
  date, sort, filter, title, subfolders).
  * Column expressions: `prop`, `rel.prop`, `backlinks.prop`, and
    `count/sum/avg/min/max/percent(...)`, with optional `Label = expr`.
  * Filters: `expr op value` joined by `and`; ops are
    `= != > < >= <= contains !contains`; `today` and `empty` are understood.
  * Edits go through the open tab's Doc when there is one, else straight to
    disk with the expected text.
  * Board lanes follow a usual workflow order.
  * Cards and calendar items drag with pointer events, not HTML5 drag and
    drop, so they also work with touch.
  * Views refresh on every vault change.
* Folder context menu → *View as database*; slash item *Database*.

## Notion-style features, Phase D — what shipped

* Synced blocks are Obsidian block embeds, `![[Note#^id]]`, so they work in
  Obsidian too. They can't be edited there.
* `embeds.ts` finds what an embed shows:
  * a block id at the end of a paragraph or list item, or on its own line
    after a table, quote or code block;
  * a heading's section;
  * the whole note without frontmatter.

  It also replaces those lines and keeps the id. The id is hidden while
  editing, so it can't be lost.
* Block handle menu → *Copy as synced block*:
  * adds an id where needed (headings use the section link instead);
  * copies the embed to the clipboard.
* Embeds in Preview have an *Edit* button: a textarea, Ctrl+Enter to save,
  Esc to cancel.
  * Saving goes through the source note's open Doc when it has one.
  * If the block changed elsewhere meanwhile, lines the other side added are
    kept.
* Embeds redraw when their note changes, whether edited in another pane or
  on disk. Block ids (` ^id`) are hidden in Preview.
* The hidden source editor no longer takes changes while Preview shows. It
  catches up when Source is shown, so a CodeMirror update can't scroll the
  shared pane.

## Rendering and visual formatting — what shipped

* Math (math.ts): markdown-it rules for `$…$`, `$$…$$` inline and `$$` blocks,
  using Obsidian's rules (no space inside the dollars, a digit after the
  closing one means currency). Rendered with KaTeX (HTML output).
* Footnotes: markdown-it-footnote. Clicks jump within the pane.
* Mermaid (codeblocks.ts): lazy-loaded on first use. The source travels
  URI-encoded in `data-mermaid`, because DOMPurify drops attribute values
  containing `-->`. Theme follows light/dark.
* Code colouring in Preview with CodeMirror's language data and
  @lezer/highlight `classHighlighter` (no extra dependency); `tok-*` colours
  in CSS.
* Raw HTML (`<h1>`, `<p style>` …) renders in Preview, sanitised.
* format.ts:
  * the toolbar: style picker, marks, links, lists, inserts;
  * a selection bubble (a CodeMirror tooltip field);
  * Mod-B, Mod-I, Mod-Shift-X and Mod-Shift-H shortcuts.

  `toggleMark` unwraps when the marks are already there; `setLineKind` toggles
  prefixes across all selected lines. Pref `toolbar` (default on).

## Diagram editor — what shipped

* `flowchart.ts`: parses Mermaid flowcharts into nodes (id, label, shape) and
  edges (from, to, label, line style), and writes them back.
  * It handles chains, `-->|x|`, `-- x -->`, `-.->`, `==>` and `---`, eight
    shapes, and quoted labels.
  * Unknown statements (classDef, style, click, comments) are kept verbatim.
  * Subgraphs, and diagrams that aren't flowcharts, are code-only.
  * Re-reading its own output gives the same text back.
* `diagram-editor.ts`: a modal with Visual and Code tabs, templates, and a
  live preview through the shared `loadMermaid()`.
  * Boxes in the preview map back via `…-flowchart-<id>-<n>` ids; arrows via
    `L_<from>_<to>_<n>`.
  * Ctrl+Enter or Ctrl+S saves. Escape only closes when nothing changed.
* Entry points:
  * an *Edit diagram* button on `.mermaid-wrap` in Preview, and double-click;
  * the slash item *Diagram*, which inserts a starter flowchart and opens the
    editor (`SlashHooks.editDiagram`).

  Saving replaces the fence body in the note.


## Bring-your-own AI — what shipped

* `src-tauri/src/ai.rs`: requests go out from Rust with reqwest (no CORS),
  streamed back as `ai-event` `{id, kind: chunk|done|error, text}`.
  * Two protocols. `openai`: `{base}/chat/completions`, Bearer key, SSE
    `choices[0].delta.content`. Used by OpenAI, Gemini (`/v1beta/openai`),
    Mistral, xAI, Groq, OpenRouter, Perplexity, all the Chinese providers,
    Ollama and LM Studio. `anthropic`: `{base}/messages`, `x-api-key`,
    `anthropic-version: 2023-06-01`, SSE `content_block_delta`.
  * Commands: `ai_config`, `ai_save_provider` (key: new, `""` removes,
    absent keeps), `ai_remove_provider`, `ai_set_default`, `ai_models`
    (GET `/models`), `ai_test`, `ai_run`, `ai_cancel`.
  * Settings in `ai.json`; keys in the OS store via keyring (Windows, macOS)
    or `ai-keys.json` (portable, Linux). Errors are rewritten into plain
    hints (bad key, unknown model, rate limit, local server not running).
* `ui/src/ai.ts`: presets (address, key link, hints), config cache, `runAi`.
* `ui/src/ai-settings.ts`: Settings → AI list and add/edit form (Load
  models, Test, More options for name, address and API style).
* `ui/src/ai-panel.ts`: one floating panel, three entry modes:
  * selection (bubble *AI* button, Ctrl+J in Source) — Replace / Insert below;
  * write at cursor (`/ai`, Ctrl+J with no selection) — Insert;
  * whole note (palette, Ctrl+J in Preview) — Add to the end.
  Streams Markdown, follow-up input keeps the conversation, Stop sits on the
  right so a late click can't insert. Without a provider it offers setup.

## Images, link previews, export — what shipped

* **Paste / drop files** (`attach.ts`, `NotePane.bindFiles`): bytes go to the
  `save_attachment` command as a raw IPC body (headers `name`, `folder`,
  URL-encoded), saved under `attachments/` with a free name. A clipboard
  `image.png` becomes `Pasted image <timestamp>.png`. Images are linked as
  `![[file]]`, other files as `[[file]]`. WebKitGTK passes no files for an
  image-only clipboard, so then `save_clipboard_image` reads it with
  `arboard` and writes a PNG. The full path is used when the bare
  name would resolve to another file. Source inserts at the cursor or drop
  point; Preview inserts after the top-level block under the pointer. A
  window-level guard stops a stray drop from navigating the webview.
* **Link previews** (`hover.ts`, `NotePane.bindHover/linkCard`): Preview
  links after 450 ms, Source wikilinks on Ctrl+hover. Shows the note (first
  160 lines), the `#section` / `#^block`, or the image; hydrated read-only.
  Leaves after 300 ms unless the pointer enters the card; Esc, typing,
  clicks and scrolling elsewhere close it.
* **Export** (`export.ts`, `NotePane.exportContent`): content is the live
  Preview clone (or a fresh render with diagrams and embeds), without
  buttons and fold carets, folds opened, with the title as an H1.
  * Print / PDF: `#print-root` plus `@media print` rules; `window.print()`.
  * HTML: only the CSS rules whose selectors match the note, fonts embedded
    as data URLs (EB Garamond latin; KaTeX and Phosphor only when used),
    vault images inlined; written by `write_export` to the path from the
    save dialog.

## Roadmap after the first release

| Phase | What | Status |
|---|---|---|
| E | Starred notes, unlinked mentions, vault find & replace, focus mode | ✅ done |
| F | Version history per note (list, compare, restore) | ✅ done |
| G | All-tasks view across the vault; global quick-capture hotkey | ✅ done |
| H | AI across the vault: ask my notes with citations, semantic search, AI database columns | ✅ done |
| I | Import from Notion, Evernote, OneNote | ✅ done |
| J | Encrypted notes and folders | ✅ done |
| K | Drawing and shapes on canvases | next |
| L | Publish a folder as a static website | |
| M | CSS snippets and plugins | |

## Phase E — what shipped

* **Starred**: `VaultSettings.starred` (vault paths, in `.marrow/app.json`).
  Tree shows a *Starred* group first; star/unstar from the row menu or the
  palette. `pathMoved` and `trashPath` keep the list in step.
* **Unlinked mentions**: `Engine::unlinked_mentions(target, names)` in
  `engine/mentions.rs` scans the in-memory note texts in parallel for the
  title and `aliases`, case-insensitive, whole words, skipping frontmatter,
  fenced code, `[[links]]`, `[md](links)`, inline code, URLs and #tags;
  returns UTF-16 columns. Inspector section with *Link*, which rewrites the
  spot as `[[Name]]` or `[[Name|as written]]` via `editNote`.
* **Find & replace**: *Replace…* in the search summary opens a second box;
  the phrase is the query minus `tag:`/`path:`/`file:`/`-word` parts;
  case-insensitive literal replace in every matching note after a
  confirmation with counts.
* **Focus mode**: `:root[data-focus]` hides the rail, sidebar, tabs,
  inspector, status bar and note header; F11, palette or *Exit*. Pref
  `wordGoal` drives a progress pill at the bottom.

## Phase F — what shipped

* `marrow-core/src/history.rs`: versions at
  `.marrow/history/<note path>/<unix ms>.md`. `before_save` keeps the text
  being replaced when the newest version is older than 5 minutes (and the
  text differs); `snapshot` forces one (deduplicated against the newest).
  Pruning: all from the last day, newest per hour to 30 days, per day to a
  year, max 500. `moved` follows renames and folder moves (case-only renames
  go through a temporary name). Tests cover the gap, renames and thinning.
* `write_note` calls `before_save`; `rename` and `move_path` call `moved`.
  Commands `history_list`, `history_read`, `history_snapshot`.
* `ui/src/history.ts`: dialog with versions grouped by day, *Changes*
  (line diff against the note now, unchanged runs folded) or *Full text*,
  *Copy text*, *Restore this version* (snapshots the current text first).
  Entry points: tree row menu, palette.

## Phase G — what shipped

* `engine/tasks.rs`: `Engine::tasks(include_done)` scans the in-memory texts
  for `- [ ]` / `- [x]` items (also `*`, `+`, `1.`, inside quotes/callouts),
  skipping frontmatter and fences. Due date from `📅`, `due:`, `due::`,
  `@due(…)` or a `[[YYYY-MM-DD]]` link; otherwise a daily note's own date
  (`fromNote`). Tests.
* `ui/src/tasks.ts`: sidebar view (rail ✓) grouped Overdue / Today /
  Upcoming / From earlier days / No date / Done; tick via `editNote`; add box
  appends `- [ ] …` under today's `## Tasks`.
* `editNote` now saves an open note straight away, so views that read the
  disk (tasks, search, mentions) see the change at once.
* Quick capture: `tauri-plugin-global-shortcut` registered from Rust
  (`capture.rs`); pressing it unminimises and focuses the window and emits
  `quick-capture` (with whether Marrow was in the background, so it
  minimises again after). `ui/src/capture.ts` box; `appendToToday` puts a
  `- HH:MM text` line under `## Log` or `- [ ] text` under `## Tasks`,
  filling an empty placeholder item if there is one. Pref `captureKey`.

## Phase H — what shipped

* `engine/recall.rs`: `Engine::chunks()` cuts notes into sections by heading
  (long ones by paragraphs, ~1200 chars), skipping frontmatter; each chunk
  has a content hash. `bm25()` ranks chunks; tokens are lower-case words, CJK
  as character pairs, minus English stop words. Tests.
* `vectors.rs`: `VectorStore` (normalised f32 vectors keyed by chunk hash,
  model tag, small binary file in the cache folder, cosine search). Test.
* `src-tauri/src/recall.rs`: `AiConfig.embed` (provider + model).
  `ai_index_build` embeds missing chunks in batches of 16 via the provider's
  OpenAI-style `/embeddings`, emitting `ai-index` progress; `ai_recall`
  embeds up to 48 new chunks on the fly, then merges BM25 and cosine
  rankings by reciprocal rank fusion (`mode: "meaning"` for search).
* UI: `aiOnVault` in `ai-panel.ts` (numbered excerpts → answer with [n]
  citations rendered as buttons, *From your notes* list, *Save as a note*);
  Settings → AI → *Search by meaning* with build/progress; search pane
  *Words / Meaning* switch; `ai-column.ts` dialog filling a property per
  note, remembered in the database block as `ai: {column: instruction}`.
* `setConfigKey` writes lists in flow style (`[a, b]`).

## Phase I — what shipped

* `marrow-core/src/import/`: converters returning files to write (nothing
  touches the vault there). Tests for each.
  * `notion.rs`: reads the export zip (nested zips too), strips the 32-hex
    ids from names, rewrites relative `[x](Page%20id.md)` links to
    `[[Page|x]]` (short name when unique), images/files to `![[path]]`,
    turns "Key: value" lines under the title into frontmatter, and makes a
    note with a ```database block per CSV, writing the CSV's columns into
    the row pages (creating pages for rows without one). Dates like
    "October 20, 2026" become ISO; "a, b" become lists. `_all.csv` skipped.
  * `enex.rs`: quick-xml parse; resources by md5 → attachments; ENML via
    htmd with placeholders for `en-media` and `en-todo`; tags, created,
    updated, source in frontmatter.
  * `docx.rs`: document.xml + rels + styles (heading levels by style name)
    + numbering (ordered vs bullets); runs merged and marked up; hyperlinks,
    tables, images.
  * `html.rs`: title, body via htmd; data: and neighbouring images saved.
  * `mod.rs`: `Output` (unique names), `clean_name`, `frontmatter`,
    `guess_value`, `tidy` (list markers, tight lists).
* `src-tauri/src/import.rs`: `import_files(kind, paths, folder)` converts on
  a blocking thread with `import-progress` events, picks a free folder name,
  writes, and tells the engine.
* `ui/src/import.ts`: the dialog (source cards with how-to-export notes,
  progress, summary with warnings, *Show the notes*).

## Phase J — what shipped

* `marrow-core/src/lock.rs`: a locked note is a `.md` file holding a `%%`
  notice and a ```` ```marrow-locked ```` fence: header line
  `v1 argon2id m= t= p= salt= nonce=`, an optional `hint <base64>` line,
  then base64 ciphertext. Argon2id (64 MiB, t=3) → 32-byte key;
  XChaCha20-Poly1305 with the header lines as associated data, a fresh
  nonce per save. Only a note that is *nothing but* the block counts as
  locked. Absurd costs from a tampered header are refused. Tests.
* Locked text is hidden from the index: `parse()` returns empty metadata and
  the text cache stores "" (with a `locked` flag, so `locked_notes()` can
  list them). Mirror sync never line-merges locked notes (both versions are
  kept). `History::forget` drops plain versions when a note is locked.
* `src-tauri/src/locks.rs`: keys by salt, in memory only. `reveal` /
  `conceal` wrap `read_note`, `write_note` (including the expected-version
  guard), `backup_for`, `history_read`, `history_snapshot` and
  `resolve_conflict`, so the window sees plain text and the disk gets sealed
  text. Locked folders: `.marrow/locked.json` with a sealed check string
  per folder (one salt per folder, so one key unlocks all its notes); new
  notes there are sealed on first save, or the save fails with
  `LOCKED_FOLDER:<folder>`. Commands `lock_info`, `locked_paths`, `unlock`,
  `lock_note`, `lock_folder`, `remove_lock`, `lock_now`; renames update the
  folder list. Argon2 crates are built optimised in debug too.
* `ui/src/lock.ts`: password dialogs (set with confirm + hint + warning;
  unlock with hint), a confirm dialog, lock-icon sets, and the idle timer
  (pref `lockAfter`, default 15 min). `openDocAsking` asks before opening;
  a save refused as LOCKED shows *Unlock and save*; *Lock all notes now*
  saves, forgets the keys and closes notes that need a password. Embeds and
  canvas cards of locked notes say so; the note header shows *Locked*.
