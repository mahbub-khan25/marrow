# Changelog

## Unreleased

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
