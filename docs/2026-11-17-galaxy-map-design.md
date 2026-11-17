# Galaxy Map for Marrow — Design

*Date: 2026-11-17 · Status: approved design, not yet built*

> MorphSlate builds the same map natively; the two apps share the look and the file format described here, so a vault looks the same in both.

## 1. What and why

Marrow already has a **network graph** (`ui/src/graph.ts`, `GraphView`): dots for notes, lines for links, laid out by d3-force. It shows *links*, but not *where things live*.

The **Galaxy Map** is a second way to look at the same vault, organized by folders:

- **The vault** is the **Milky Way**, at the center.
- **Each folder** is a **galaxy**. Subfolders are galaxies linked to their parent galaxy.
- **Each note** is a **planet** orbiting its folder's galaxy.
- **The user can draw connections** between any two galaxies. A connection is **only a line on the map**: a visual aid for finding related areas quickly. It never moves files or edits notes.

It must be **visually gorgeous** and come in two looks the user can switch between at any time. Each look has a light and a dark version, and **light is the primary design target**:

- **Orbit:** clean and modern. White (or near-black) background with a faint dot grid, thin orbit rings, solid colorful planets, a colored core per galaxy, and bold sans-serif labels with "N notes" under each galaxy.
- **Celestial Atlas:** an antique star chart.
  - Cream paper (light) or midnight navy (dark).
  - A faint graticule (concentric ellipses and radial lines centered on the Milky Way).
  - Galaxies drawn as a double ink circle with slowly turning compass ticks and a gold star at the center.
  - Planets as small ink four-point stars, with a galaxy's notes joined like a constellation.
  - Italic serif labels.
  - User connections in dotted gold.

The approved reference mockup is `docs/galaxy-styles-reference.html` (options B and C); open it in a browser. Its palettes are the starting palette values (§6).

## 2. Goals, non-goals, anti-goals

**Goals**
- A Galaxy layout beside the existing Network layout in the graph panel, switchable with one click and remembered.
- Two themes (Orbit, Atlas), each in light and dark, following Marrow's current light/dark setting.
- User-drawn galaxy connections, saved in the vault so they travel with it, including into MorphSlate.
- Smooth on large vaults: 60 fps with 5,000 notes on a mid-range laptop.
- Fully usable from the keyboard, and readable by screen readers.

**Non-goals**
- Moving or renaming files from the map. Dragging a planet into another galaxy does **nothing to files**.
- Replacing the Network graph. It stays as is.
- 3D, WebGL or custom shaders. Canvas 2D, like the existing graph.

**Anti-goals**
- The map changing note contents or files.
- Visual noise: every note-to-note link drawn at once.
- Burning battery when the panel isn't visible.

## 3. Fit into Marrow's current code

| Existing piece | How the Galaxy Map uses it |
|---|---|
| `GraphPanel` (`ui/src/graphpanel.ts`) | Gains a **Layout** switch: `Network` / `Galaxy`. Toolbar, filter box, minimap, zoom and Fit buttons are reused. The forces drawer is hidden in Galaxy mode. |
| `GraphView` (`ui/src/graph.ts`) | Untouched. A sibling class `GalaxyView` implements the same `GraphViewOptions` callbacks (`onSelect`, `onOpen`, `onView`) and the same zoom, fit and pan methods, so the panel can host either. |
| `graph` Tauri command → `GraphData` | Supplies notes (`kind: "note"`), their vault-relative ids (paths) and edges. Folders come from note paths. Empty folders come from the vault tree the sidebar already uses. |
| Vault events (`created`, `deleted`, `renamed`) | Galaxies and planets update live; renamed folders keep their connections (§5). |
| Settings (`ui/src/settings.ts`) | New Graph settings: default layout, map theme (`orbit`/`atlas`), motion (shared with the existing Motion toggle). |
| Reduced motion | Same rule as the existing graph: motion is off by default when the system asks for reduced motion. |

New files:
- `ui/src/galaxy/layout.ts` (pure layout)
- `ui/src/galaxy/view.ts` (`GalaxyView`)
- `ui/src/galaxy/themes.ts` (palettes and painters)
- `ui/src/galaxy/mapfile.ts` (connections file)
- tests beside each

## 4. Layout (deterministic, no physics)

A galaxy map must not wobble or reshuffle between openings. **The same vault always gives the same picture**, so layout is a pure function of the folder tree:

```ts
layoutGalaxies(tree: FolderNode, pinned: Record<string, Point>, size: Size): GalaxyLayout
```

**Galaxies:**
- **Milky Way:** the vault root at the origin.
- **Top-level folders:** placed on an ellipse around it, in alphabetical order, evenly spaced, starting at the top.
- **Subfolders:** fanned outward from their parent, away from the center, in a spread proportional to their count. This repeats per depth with shrinking distances.
- **Size:** galaxy core radius = `clamp(14 + 2.2·√notes, 14, 40)`. The Milky Way is 1.5× that.
- **Overlap:** a single collision pass separates overlapping galaxies (d3-force `forceCollide` with ~30 ticks, run once, not live).

**Planets:**
- Notes orbit on 2–5 elliptical rings (vertical squash 0.82, giving a tilted-orbit feel).
- A note's ring and starting angle come from a hash of its path. Its size is set by its backlink count (`3 + min(5, √backlinks·1.6)` px).
- **Motion:** orbit speed is proportional to `1/ring radius` (inner planets move faster), very slow: one orbit takes 60–180 s.

**Pinning:** the user can drag a **galaxy** to rearrange the map. The position is saved in the map file (§5), and its subgalaxies move with it. Dragging a planet only previews its note; nothing is saved.

**Level of detail:**
- **Zoomed out:** galaxies with more than 40 notes show 40 planets plus a soft dust ring and a "+N" badge.
- **Labels:** galaxy labels are always on. Planet labels appear on hover or focus, or at zoom ≥ 1.6.
- **Note-to-note links:** drawn **only** for the hovered or selected planet, as faint arcs to its linked planets, even across galaxies.

## 5. The map file (shared with MorphSlate)

**Location:** `<vault>/.galaxy-map.json`. It's a hidden dotfile, so Obsidian ignores it and it syncs with the vault. It's written atomically (temp file, then rename). Reading tolerates unknown fields.

```json
{
  "version": 1,
  "connections": [
    { "a": "Physics", "b": "Programming", "label": "simulations" }
  ],
  "pinned": { "Physics": { "x": -320, "y": -140 } }
}
```

**Folder references:**
- **Format:** folders are vault-relative paths with `/` separators, and `""` means the Milky Way.
- **Rename:** when a folder is renamed or moved in Marrow, Marrow rewrites matching `a`/`b`/`pinned` keys, so connections survive. Marrow already gets `renamed` events with `from`/`to`.
- **Deleted folders:** a connection pointing to a folder that no longer exists is hidden, not deleted. It comes back if the folder returns. "Clean up missing" in the map menu removes such connections.

**Not stored in the file:** the theme and light/dark mode are personal preferences in app settings, not vault data.

## 6. Themes

Each theme is a palette plus a painter. Both painters take the same layout and draw in this order:

1. background
2. grid or graticule
3. hierarchy lines
4. user connections
5. galaxies
6. planets
7. labels
8. hover card

**Starting palettes** (from the approved mockup):

| | Orbit · light | Orbit · dark | Atlas · light | Atlas · dark |
|---|---|---|---|---|
| Background | `#ffffff` + dot grid `rgba(30,35,60,.06)` | `#0d0f14` + grid `rgba(255,255,255,.05)` | radial `#f7f1e3`→`#efe5cf` | radial `#0f1c36`→`#091224` |
| Rings / graticule | `rgba(30,35,60,.12)` | `rgba(255,255,255,.12)` | `rgba(34,48,77,.10)` | `rgba(234,223,196,.08)` |
| Hierarchy lines | `rgba(30,35,60,.22)` straight | `rgba(255,255,255,.22)` | ink 35% curved | ink 35% curved |
| User connections | accent `#5b5bd6`, dashed 5/6, 2px | `#8f8fff` | gold `#b5852f`, dotted round caps 2/5, 2.2px | gold `#dcb66b` |
| Labels | `#1d1f27` 600 13px sans; root 700 16px | `#eef0f6` | ink `#22304d` italic Georgia 14px; root 600 18px | `#eadfc4` |
| Planets | solid HSL by galaxy hue, L 55%, with a 2px background-colored rim | L 64% | ink four-point stars | cream stars |

**Galaxy hue:** each top-level galaxy gets a hue from a fixed, color-blind-safe set. Subfolders take their parent's hue, shifted +25° per level.

**Fonts:** Marrow already bundles EB Garamond and Literata, so the Atlas theme uses **EB Garamond** instead of Georgia for a consistent look across systems.

**Contrast:** all label colors must meet WCAG AA against their background.

## 7. Interaction

| Action | Mouse | Keyboard |
|---|---|---|
| Pan / zoom | Drag background / scroll | Arrow keys pan, `+`/`-` zoom, `0` Fit |
| Focus a galaxy | Click | Tab / Shift+Tab cycles galaxies; arrows move to the nearest galaxy in that direction |
| Enter a galaxy (zoom to it) | Double-click galaxy | Enter |
| Focus a planet | Hover / click | Inside a galaxy: Tab cycles planets |
| Open a note | Double-click planet, or the card's Open button | Enter on a focused planet (Shift+Enter opens in split) |
| Draw a connection | Alt+drag from one galaxy to another | `C` on a focused galaxy, choose the target with Tab or arrows, Enter |
| Label / remove a connection | Right-click the line | Focus the line (Tab reaches connections after galaxies), F2 relabel, Delete remove |
| Move a galaxy | Drag it | Alt+arrows (Shift for bigger steps) |
| Find | `/` or Ctrl+F filter box (existing); Enter flies to the match | Same |
| Switch theme | Theme chip in the toolbar (Orbit / Atlas) | Command palette: "Galaxy map: switch theme" |
| Back | Esc leaves a galaxy, or returns to the note | Same |

**Hover card:** reuse the existing graph card (folder, tags, excerpt, incoming and outgoing counts, Open / Open in split).

## 8. Motion and performance

- **Animated elements:** orbits turn slowly; Atlas compass ticks drift (0.03 rad/s); dashed Orbit connections don't move.
- **Pausing:** the existing graph's rules apply. Drawing pauses while the panel is hidden, and motion is off when reduced motion is set or the Motion toggle is off. With motion off, frames are drawn only on interaction.
- **Caching:** the static layers (background, grid or graticule, hierarchy lines, galaxy outlines) are cached in an offscreen canvas and redrawn only on pan, zoom, resize, theme change or data change. Planets and labels draw each frame.
- **Budget:** 60 fps at 5,000 notes; under 4 ms per frame on the reference laptop. Beyond 20,000 notes, galaxies show dust rings and counts only, until the user enters one.

## 9. Accessibility

- **Screen readers:** a visually hidden DOM tree mirrors the map (`role="tree"`). Galaxies are tree items: "Physics, galaxy, 8 notes, 1 subfolder, connected to Programming". Their planets are child items: "Entropy, note, 3 backlinks". Focus in the canvas and focus in the tree stay in sync, so screen readers announce what is focused.
- **Visible focus:** a focus ring is drawn on the focused galaxy, planet or connection.
- **Color:** hues are never the only signal. Connections differ by dash pattern, and galaxies are labelled.

## 10. Testing

- **`layout.test.ts`:**
  - same tree → identical positions (determinism)
  - children are placed farther from the center than their parent
  - pinned galaxies stay put and move their subgalaxies with them
  - no overlapping galaxy cores after the collision pass
  - a folder with 0 notes still gets a galaxy
- **`mapfile.test.ts`:**
  - round-trip the JSON
  - unknown fields are kept on write
  - a missing file means an empty map
  - a corrupt file means an empty map plus a warning, and is never overwritten without a backup
  - a folder rename rewrites keys
- **`view.test.ts`** (jsdom): keyboard focus order (galaxies → connections), `C` → target → Enter creates a connection, Esc behavior.
- **Manual checks:**
  - both themes × light and dark × reduced motion
  - a 5,000-note demo vault, at 60 fps
  - NVDA and VoiceOver walkthroughs of the hidden tree

## 11. Left to the builder

- The exact hue set and the subfolder fan-angle constants.
- The toolbar icon for the Layout switch.
- Whether the Galaxy layout gets its own minimap rendering or reuses the existing one at a lower detail level.

## 12. Decisions

| Decision | By |
|---|---|
| Folders = galaxies, notes = planets, vault = Milky Way, subfolders linked to parent | Owner |
| Connections are map-only lines; the map never moves files | Owner |
| Two themes, Orbit and Celestial Atlas, switchable; light and dark; light preferred | Owner |
| Galaxy Map also in Marrow; this document is separate from MorphSlate's | Owner |
| Layout is deterministic (no live physics); galaxies can be pinned by dragging | Claude's proposal |
| Map file `.galaxy-map.json` at the vault root, shared with MorphSlate | Claude's proposal |
| Galaxy view is a sibling of `GraphView` inside the existing `GraphPanel` | Claude's proposal |
| Note-to-note links shown only for the hovered or selected planet | Claude's proposal |
