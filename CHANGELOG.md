# Changelog

## Unreleased

- **Delete a drawing in one step:** a bin button on every drawing (on
  computers it shows when you point at the drawing; on phones and tablets
  it's in the drawing's toolbar). After asking, it takes the drawing out of
  the note and moves its file to Marrow's trash, where you can restore it.
- **Every drawing tool fits on phones:** the drawing toolbar wraps onto a
  second line instead of hiding tools off the edge, and the drawing keeps
  its full size while you draw. The eraser has its own icon now, so it isn't
  mistaken for delete.

## 1.7.1

Move what you draw on phones and tablets, and the new Marrow logo everywhere.

- **The new Marrow logo on phones and tablets:** the Android and iPhone/iPad
  apps now use the same wings logo as the computer app, on the home screen,
  the welcome screen and in notifications. On Android 13 and later it also
  follows your themed-icons colour.
- **Move what you've drawn on phones and tablets:** the new **Select and
  move** tool (the hand) picks a shape, line, arrow, words or pen stroke.
  Drag it to move it, drag its corner to resize it, and change its colour or
  fill, edit its words or delete it. A shape, line or arrow you've just drawn
  is picked straight away, so you can move it at once. See-through shapes are
  picked by their outline, so what's under them can still be picked.

## 1.7.0

More ways to draw, see-through shapes, and a window you can arrange your way, on computers and tablets.

- **More to draw with:** 11 shapes (rectangle, rounded rectangle, circle,
  diamond, triangle, hexagon, parallelogram, star, cylinder, speech bubble
  and cloud), plus a straight line, a double-headed arrow and a highlighter.
- **See-through shapes:** pick **No fill** to lay a shape over others and
  still see what's under it, or **Solid** for a filled shape. The usual
  light fill is still there.
- **Hide or resize the parts of the window** (computers): hide the icon bar,
  the sidebar, the inspector or the status bar, each on its own (buttons in
  the tab bar, Settings › Appearance, or the command palette;
  `Ctrl+Shift+[` and `Ctrl+Shift+]` for the sidebar and inspector). Drag the
  edge of the sidebar or the inspector to make it wider or narrower;
  double-click the edge to put it back. Marrow remembers your choices.
- **Tablets:** on an Android tablet or in Android desktop mode, drag the
  line beside the sidebar to make it wider or narrower (double-tap to put it
  back). On iPad, wide windows now have a sidebar instead of tabs; hide it
  with its button or ⌘\, and drag its edge to resize. Both remember your
  choice.

## 1.6.0

Draw inside your notes, save web pages, get reminded, and see today's tasks on your home screen.

- **Drawings inside notes:** type `/drawing` (or use the pencil button)
  and a drawing area opens right where you're writing. Sketch with the pen,
  rectangles, circles, diamonds, arrows and words, in six colours.
  - On phones and tablets, tap a drawing to draw on it with your finger, a
    stylus or Apple Pencil, then tap **Done**. "Drawing" in the editor bar
    adds a new one.
  - Each drawing is saved as a small canvas next to your notes, so it syncs
    and keeps its history. Any canvas can be shown in a note with
    `![[Name.canvas]]`.
- **Save web pages (web clipper):** the new **Marrow Clipper** browser
  extension (Chrome, Edge, Brave, Firefox) saves the article you're reading,
  or just the part you selected, as a note in **Clippings**, without the
  menus and ads. Download `Marrow_Clipper_1.6.0.zip` below; the steps to
  install it are in its README. Marrow must be open on the same computer.
- **Share a link to Marrow** on Android or iPhone to save the whole page in
  Clippings. On iPhone and iPad, "Add to Marrow" is now in the share sheet,
  for pages, words and photos.
- **Reminders on tasks:** tap the alarm on a task (or "Remind me" while
  writing) and pick in an hour, this evening, tomorrow morning, or any date
  and time. The time is kept in the note as `⏰ 2026-11-03 09:00`, so it
  syncs like everything else. Notifications have **Done** and **Snooze
  10 min** buttons.
  - On Android, iPhone and iPad they arrive even when Marrow is closed.
  - On computers they arrive while Marrow runs. It can keep running in the
    tray and start when you sign in (Settings › Editor › "Keep running for
    reminders"); missed reminders from the last day ring when it opens.
- **Home-screen widget (Android, iPhone and iPad):** what's due today and
  overdue, with a tick box for each task and a button to add one. Tap a task
  to open its note.

## 1.4.0

Marrow comes to Android, and syncs through the cloud storage you already have.

- **Marrow for Android:** download `Marrow_1.4.0_android.apk` below and open
  it on your phone or tablet (allow "Install unknown apps" when Android
  asks). Today and daily notes, the calendar, tasks, search, quick capture
  with a photo or by speaking, locked notes, the graph, canvases, version
  history, Ask your notes, English or Bangla.
- **Desktop layout on Android tablets:** in Android's desktop mode, on an
  external screen or on a tablet in landscape, a sidebar shows your folders
  and notes beside the page. Keyboard shortcuts: Ctrl K search, Ctrl D
  today, Ctrl N new note, Ctrl \ sidebar. Right-click works like a long press.
- **Sync with Google Drive, OneDrive and Dropbox**, on computers and
  phones: sign in once, pick the vault's folder, and your notes stay the same
  everywhere, free with the storage you already have. Notes stay plain
  Markdown files in *My Drive › Marrow* or *Apps › Marrow*, and Marrow can't
  see anything else in your account. End-to-end encryption works with them
  too.
- **iPhone and iPad:** the app is built and tested, with iCloud sync, and
  comes to the App Store and TestFlight next. See
  [apps/README.md](apps/README.md).
- The lock and WebDAV code moved into the engine crates so the phone apps
  use the same code as the desktop app.

## 1.3.0

A new look that is easier to read and to find your way around.

- **New type:** the interface uses Atkinson Hyperlegible Next, made by the
  Braille Institute so letters like I, l and 1 can't be mistaken for each
  other. Notes use Literata, made for long reading on screens. Choose
  Book, Clean or Classic (the old Garamond) in Settings → Appearance →
  Note text.
- **Every place has a name:** the side bar shows a name under each icon
  (on phones too). Today's note has its own sun icon, so it no longer looks
  like the calendar.
- **Help:** a new Help button explains every part of Marrow in one plain
  sentence each, with the keys worth knowing.
- **Start page:** opens with today's date, your notes and what to do next,
  with a short "Getting started" while a vault is new.
- **Nothing overlaps any more:** the note header on phones, the side panel
  on smaller laptop screens (it now opens over the note only when asked),
  tab names on phones, and settings whose choices are wide.
- **Calmer, clearer screens:** softer colours with the note as the
  brightest thing, rounded controls, plain sentence-case labels, a title
  that no longer repeats the note's first heading, and settings in plain
  words (Small, Medium, Large instead of pixel sizes).

## 1.2.0

- **Time machine:** drag a slider to see a note, or the whole vault, as it
  was on any past date. See what changed since, restore an old version, or
  bring back a note that was deleted. Built on the versions Marrow already
  keeps while you write.
- **Notes come back to you:** the start screen shows a note you haven't
  opened in a long time and the same day in earlier years. From Friday to
  Sunday it offers a five-minute weekly review: what you wrote, what's
  still open, loose ends, and a few questions, saved as a note if you like.
  Turn it off in Settings → Editor.
- **Move from Notion or Evernote, step by step:** exactly how to export,
  the export found in Downloads automatically (or dropped on the window),
  then everything brought in with its links, pictures and databases.
- **Bangla:**
  - The whole interface in Bangla (Settings → Appearance → Interface
    language). Your notes are never changed.
  - Search that understands Bangla spelling: ি/ী, ু/ূ, ন/ণ, শ/ষ/স, জ/য,
    ং/ঙ, ড়/র and ৎ/ত match each other, and Bangla digits match 0-9.
  - A Bengali font that matches Marrow's type, in notes and in PDF exports.

- **Sync with your own server:** Nextcloud, ownCloud, Synology, pCloud, Koofr
  and other WebDAV servers, on any device. Same safe two-way sync as cloud
  folders: notes edited in two places are merged, deletions go to the trash,
  and sync pauses rather than empty a vault. Being offline is fine; sync
  resumes by itself.
- **End-to-end encrypted sync:** optional, for cloud folders and servers.
  Notes and their names are encrypted before they leave the device, so the
  cloud stores only scrambled data. Other devices join with the passphrase.
- **Ask your notes:** an Ask button in the sidebar and on the start screen,
  one-click private setup with Ollama on this computer, and a clear note of
  where each question goes. Also fixes the Ask panel opening off-screen.
- **Quick capture with voice and photos:** record a voice note or take a
  photo straight into today's note. With an OpenAI or Groq key in
  Settings → AI, voice notes are also turned into text; the recording is
  always kept. Recordings play right inside the note. On phones and
  tablets, a round + button opens Quick capture from anywhere.
- **Mobile groundwork:** Marrow builds for Android and iOS, with layouts for
  phones and tablets.

## 1.1.0

- **Calendar:** a new view in the sidebar. See a month of daily notes and the
  days tasks are due, open or start any day's note in one click, or see the
  whole year at a glance.
- **On this day:** the same date in earlier years, with the first line of
  each note.
- **Daily notes by year and month:** new daily notes go into
  `Daily/2026/10 October/2026-10-24.md`. Older notes in one folder keep
  working, and one click moves them into place with every link intact.
  Choose *One folder* in Settings → Vault to keep the old way.
- Previous/next day and the Tasks view work across all of these folders.

## 1.0.0

The first stable release.
- Windows installers and the app are code-signed, so Windows shows the
  verified publisher instead of "Unknown publisher".
- Includes every fix from the 0.9.0 betas: crash-safe saving, drafts of
  unsaved typing, crash-safe renames and locked folders, and canvas
  conflict handling.
- Marrow is free and open source under the MIT license.

## 0.9.0-beta.2

Fixes from the third readiness review:
- A canvas save now also covers edits made while it was writing, before
  Marrow closes the canvas or switches to another one.
- The "Not saved" warning on closing now says so when a canvas's changes
  couldn't be kept either, instead of promising they were.
- Moving a note into a locked folder protects its kept unsaved text before
  the note itself, and a retried move finishes anything an earlier try
  left readable.
- Renaming or moving a locked folder keeps it locked even if Marrow stops
  halfway: it's listed under both names until the move is done, and the
  next open tidies up.

## 0.9.0 — release candidate (beta)

The first version meant for other people. No new features: this release is
about never losing a note, keeping locked notes private and staying fast.

### Your notes are safe
- Every save is crash-safe: the new text is written to a temporary file,
  flushed to the disk and then swapped in. Tested by killing Marrow 200 times
  mid-save on Windows and Linux: no damaged notes (an ordinary overwrite
  damaged 35 on Windows and 163 on Linux).
- Unsaved typing is kept as a draft, outside the vault: half a second after
  you pause, and at least every 5 seconds while you keep typing (notes are
  saved at least every 15 seconds). After a crash or power cut, Marrow
  offers it back.
- Marrow won't close, switch vaults, close a tab, lock or trash anything over
  text it couldn't save. It asks first, and keeps the text if you go on.
- A rename that's interrupted is finished the next time the vault opens,
  including updating links; notes edited in the meantime are left alone.
- Rename only rewrites a link that is still exactly where it was, and only
  replaces a note if nobody changed it meanwhile, like an ordinary save.
- Moving a note to the trash saves it first, and closes it only once the
  move has worked.
- Canvases check for changes made in another app before saving. If both
  changed, both versions are kept, including edits made while that happens.
  A canvas whose changes can't be saved or kept stays open.
- Opening Marrow again brings the open window forward instead of starting a
  second copy.
- Rebuilding the portable version never deletes its Notes or Data folders.

### Locked notes stay private
- Notes created, imported or moved into a locked folder are encrypted from
  the first byte (a note being moved is encrypted before it moves, along
  with any unsaved text kept for it).
- If the list of locked folders can't be read, Marrow refuses to save rather
  than guessing that nothing is locked.
- Locking a note also removes its readable earlier versions, backups,
  drafts and cached headings, and says which copies couldn't be removed.
- A published website always leaves locked notes out, even while they're
  unlocked for reading.

### Fast and responsive
- The window stays responsive: all work happens in the background. (Very
  large operations, such as renaming a note thousands of others link to,
  still make other note actions wait until they finish.)
- Database views, backlinks and the inspector use a new link index:
  opening a 2,000-row database went from 8 s to 0.04 s.
- Saving is about three times faster in large vaults.

### Small fixes
- Diagrams redraw in the right colours after switching light/dark.
- Website search results no longer show callout markup like `[!tip]`.
- Property types you pick (date, number…) are remembered after a restart.

### For support
- A small diagnostic log (no note text, no passwords): Settings → About →
  Show the log.
