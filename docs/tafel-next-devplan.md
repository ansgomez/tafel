# Tafel Next — Development Plan

Status: plan only, nothing implemented.

This plan is for a future, cross-platform rewrite of Tafel (Linux, Windows, macOS, Android,
iPad). It is a separate project from this repository: it uses no Rnote code and is planned
under the MIT license (© Andres Gomez). The Tafel app in this repository stays a GPL-3.0
fork of Rnote.

## 1. Goal

One PDF annotation app that looks and behaves like the MarpX annotation mode, and runs on
the Framework 12 (Linux), Windows, macOS, Android and the iPad.

- Open a PDF, show one page at a time fitted to the window, turn pages by button, keyboard or
  two-finger swipe.
- Marp toolbar and tools: pen, highlighter, selector (tap / rectangle / lasso), laser,
  eraser, partial eraser, 8 colors, 4 sizes, undo/redo, clear page, blank pages with
  plain / lined / grid paper, page overview, dockable bar.
- Notes saved next to the PDF and exported as an annotated PDF.

Non-goals for v1: cloud sync, collaboration, text recognition, typed text boxes, shapes,
infinite canvas.

## 2. Starting material

### Marp Notes (the annotation mode in the lecture HTML)

The annotation mode embedded in the Marp decks (e.g.
`~/Dropbox/tubs/lectures/ti1/exercise/exercise1_widerstandsnetzwerke/slides/exercise1.html`,
lines ~1165–4381) is a set of plain JS modules:

| Module | Content | Reuse |
|---|---|---|
| `geometry.js` | Smoothing, pressure width, stroke outline, bboxes, hit tests, partial erase, lasso, laser trail. Pure, no DOM. | As is |
| `model.js` | Immutable pages/strokes, undoable `NotesDoc` commands, history, blank pages. | As is |
| `tools.js` | Pen, highlighter, erasers, selector, laser, tool registry. Edit only through `NotesDoc`. | As is |
| `input.js` | Pointer routing, pen vs. touch, two-finger swipe. | Small changes |
| `render.js` | One SVG `<g class="ann-layer">` per Marp slide SVG. | Adapt to PDF pages |
| `pages.js` | Page order, blank pages, counter, Marp hash navigation. | Rewrite navigation part |
| `overview.js` | Thumbnail grid, `G` + number jump. Clones Marp slide SVGs. | Rewrite thumbnails |
| `io.js` | `marp-notes` JSON v1 save/load, print to PDF. | Adapt (file system, export) |
| `sync.js` | Presenter view mirroring. | Later phase |
| `toolbar.js` | The Marp bar, groups, dock, icons, CSS. | As is |
| `main.js` | Wiring. | Rewrite |

The source modules were not found on disk; only the bundled HTML is. **Step 0.4** extracts
them. If a source repo exists, use it instead.

Strokes are stored as `{ id, tool, color, size, points: [[x, y, pressure], ...] }` in slide
viewBox units. That point format is exactly what perfect-freehand consumes.

### Libraries

| Library | Use | License |
|---|---|---|
| Tauri 2 | Native shell for desktop, Android, iOS | MIT / Apache-2.0 |
| PDF.js (`pdfjs-dist`) | PDF rendering in the web view | Apache-2.0 |
| perfect-freehand | Pressure-sensitive stroke outlines | MIT |
| pdf-lib | Writing annotated PDFs (export) | MIT |
| Vite | Dev server and bundling | MIT |

### Licensing

- The MarpX theme is MIT (© Paulo Cunha). Keep its notice where its CSS is used.
- The Marp Notes JS modules are written by Andres Gomez and are relicensed as MIT in Tafel.
- Tafel is MIT. No GPL code is used (unlike the Rnote fork), so it can go to the App Store
  if wanted. Keep the notices of all bundled libraries (`THIRD_PARTY_NOTICES.md`).

## 3. Architecture

```
┌──────────────────────── Tauri 2 window (web view) ────────────────────────┐
│  Toolbar (toolbar.js)                                                     │
│  ┌──────────────── Page view (one page) ─────────────────┐                │
│  │ <canvas>  PDF.js render of page n (or blank paper)    │                │
│  │ <svg>     annotation layer, viewBox = page size in pt │ ← render.js    │
│  │ <canvas>  live stroke + laser (low latency)           │                │
│  └───────────────────────────────────────────────────────┘                │
│  input.js → tools.js → NotesDoc (model.js) → render.js                    │
│  io.js ── platform adapter ──┬── Tauri: fs plugin, dialogs                │
│                              └── PWA: file input / download / OPFS        │
└───────────────────────────────────────────────────────────────────────────┘
Rust side: open/save dialogs, file read/write, recent files, window state. No rendering.
```

Key decisions:

- **Coordinates:** each PDF page's annotation SVG uses a viewBox in PDF points
  (`page.getViewport({ scale: 1 })`). This plays the role of the Marp slide viewBox, so
  `geometry.js`, `model.js` and `tools.js` work unchanged. Pages can have different sizes.
- **Page ids:** PDF pages are `p1 … pN`; blank pages keep the existing uuid ids and their
  position after a page id, as in `model.js`.
- **Rendering:** render the current page at `devicePixelRatio × fit zoom`; pre-render the
  next and previous page into `ImageBitmap`s; keep at most ~6 bitmaps. Re-render once after
  a pinch zoom ends.
- **Latency:** the stroke being drawn goes to a `desynchronized` 2D canvas using
  `getCoalescedEvents()` and `getPredictedEvents()`; it is moved into the SVG on pen up.
- **perfect-freehand:** `geometry.strokeOutline()` becomes a thin wrapper around
  `getStroke(points, options)` with options tuned to look like today's Marp pen
  (`thinning ≈ 0.6`, `streamline ≈ 0.5`). Keep the old outline behind a flag until the
  look is approved. The highlighter stays a constant-width stroke.
- **Platform adapter:** `io.js` talks to a small interface (`openPdf`, `readNotes`,
  `writeNotes`, `exportPdf`). Tauri and the PWA each implement it, so the same front end
  ships as a native app and as a web page.

## 4. File formats

- **Notes:** sidecar `<name>.pdf.notes.json`, `format: "marp-notes"`, `version: 2`.
  - v2 adds `deck.source: "pdf"`, `deck.sha256` of the PDF and per-page sizes.
  - Load v1 files from the HTML decks unchanged (deck title and slide count warnings are
    kept from `io.js`).
  - Autosave 2 s after the last change and on page turn; write to a temp file, then rename.
- **Export:** pdf-lib copies the original PDF, inserts blank pages with their paper, and
  draws every stroke as a filled vector path (the perfect-freehand outline). Highlighter
  uses the PDF's transparency (`opacity` in the graphics state). The original PDF is
  never modified.

## 5. Phases

Effort is rough, in working days for one developer with Claude.

### Phase 0 — Spikes and extraction (2–3 days)

Goal: remove the big unknowns before building anything.

1. **WebKitGTK pen input on the Framework:** pressure in `PointerEvent.pressure`,
   `pointerType === "pen"`, coalesced events, latency. WebKitGTK is the Linux web view of
   Tauri and is the biggest risk.
2. **PDF.js performance:** open the 37-page `exercise1.pdf`, page-turn time, memory with
   bitmap cache, on the Framework and on an iPad (Safari).
3. **iPad pencil in Safari:** pressure, palm rejection, two-finger swipe vs. pinch.
4. **Extract Marp Notes modules** from the bundled HTML into `src/notes/*.js` as ES modules,
   with their tests if they exist.

Exit: a written note per spike. If WebKitGTK input is not good enough, choose a fallback
(keep the Rnote fork on Linux, or a Chromium-based shell for Linux only).

### Phase 1 — Viewer (3 days)

- Tauri 2 + Vite skeleton; also runs as plain web page (`vite dev`).
- Open a PDF (dialog, drag and drop, command line argument, file association).
- Single-page view, fit to window, refit on resize.
- Page turn: toolbar buttons, keys (←/→, PgUp/PgDn, Space), two-finger swipe.
- Page counter "3 / 37".

Acceptance: `exercise1.pdf` opens fitted at page 1, page turns are under 100 ms with the
cache warm.

### Phase 2 — Ink (4 days)

- Annotation SVG layer per page, `render.js` adapted.
- `input.js`: pen draws, touch pans and zooms, palm rejection while the pen is near.
- Tools from `tools.js`: pen, highlighter, eraser, partial eraser, selector, laser.
- perfect-freehand integration, low-latency live stroke.
- Undo/redo, clear page.

Acceptance: drawing on the Framework and on the iPad feels as good as the Marp HTML version.

### Phase 3 — Toolbar and look (2 days)

- `toolbar.js` and its CSS unchanged; dock on all four edges; colors, sizes, icons.
- Dark bar, the same keyboard shortcuts as MarpX.

Acceptance: side-by-side screenshot with the MarpX deck shows the same bar.

### Phase 4 — Pages (3 days)

- Blank pages after the current page, plain / lined / grid paper, delete blank pages.
- Counter "3 / 37 · +2 blank".
- Overview grid with PDF thumbnails (small PDF.js renders, cached); `G` + number jump.

### Phase 5 — Persistence (3 days)

- Sidecar notes, autosave, crash-safe writes, "unsaved" indicator.
- Load v1 notes from the HTML decks.
- Recent files, window state.

Acceptance: kill the app while drawing; on reopen, at most the last 2 s are lost.

### Phase 6 — Export (3 days)

- Annotated PDF via pdf-lib, vector strokes, blank pages inserted.
- Print via the system dialog.

Acceptance: the exported PDF looks the same in Evince, Okular and Acrobat, and its text can
still be selected.

### Phase 7 — Packaging (3–5 days)

- Linux: AppImage and RPM (Fedora), optional Flatpak.
- Windows: MSI. Android: APK.
- iPad, two routes:
  - **PWA** (no Apple account): same front end, installed from Safari, offline via
    service worker, files via file picker and download / OPFS.
  - **Native iOS** via Tauri: needs a Mac with Xcode and an Apple developer account
    (99 USD/year) for installs that do not expire after 7 days.

### Phase 8 — Later

- Presenter mode with `sync.js`: second window on the projector mirrors page and ink, laser
  visible to the audience.
- Pen-hover laser, slide overview while presenting.
- Optional: open Marp HTML decks directly (render slides instead of PDF pages).

Total for v1 (phases 0–7): about 23–26 days.

## 6. Testing

- Unit tests (Vitest) for `geometry.js`, `model.js`, `tools.js` and `io.js`
  (format v1/v2 round trips).
- End-to-end tests (Playwright, Chromium and WebKit) with synthetic pen pointer events
  including pressure: draw, erase, select, undo, page turn, blank pages, save and reload.
- Manual matrix per release: Framework 12 (Fedora, pen and touch), iPad (Pencil), one
  Android tablet, Windows.

## 7. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| WebKitGTK pen pressure or latency is poor | Linux, the main device, feels worse than Rnote | Spike first (0.1); fallback to the Rnote fork or a Chromium shell on Linux |
| Large PDFs (100+ pages, scans) use too much memory | Crashes on iPad | Bitmap cache limit, render at screen size only, release pages off screen |
| perfect-freehand looks different from the Marp pen | Old notes change look | Keep old outline behind a flag; tune options; decide in phase 2 |
| iOS file access is limited | Sidecar notes awkward on iPad | Store notes in app storage keyed by PDF hash; import/export buttons |

## 8. Open decisions

Decided: name **Tafel**, MIT license, local repo at `~/git/tools/tafel-next`.

1. Repo location: local only or GitHub.
2. Plain JS (as the Marp Notes modules are) or TypeScript for new code.
3. iPad route: PWA first, or native iOS from the start.
4. Keep developing the Rnote fork in parallel, or freeze it once phase 5 is done.
