# Changelog

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
