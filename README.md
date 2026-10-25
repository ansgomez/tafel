<div align="center">
<img src="crates/rnote-ui/data/icons/scalable/apps/tafel.svg" width="160" alt="Tafel icon">

# Tafel

**Annotate PDF slides with a pen, the way the MarpX annotation mode does.**

*Tafel* is German for blackboard.

</div>

---

> [!IMPORTANT]
> **Tafel is a modified fork of [Rnote](https://github.com/flxzt/rnote)** by Felix Zwettler and
> the Rnote contributors. Rnote does all the hard work: the drawing engine, stroke rendering,
> PDF import and export, the file format and much of the GTK app.
> Tafel only replaces the user interface and adds a slide-focused workflow on top.
> Tafel is not affiliated with or endorsed by the Rnote project. Please report Tafel bugs here,
> not upstream.

## Why

[MarpX](https://github.com/cunhapaulo/MarpX) slide decks can carry an annotation mode: a slim
dark toolbar, one slide at a time, a pen, a highlighter, a laser pointer, and blank pages for
working through an example during a lecture. Tafel brings that same experience to **any PDF**,
as a native app for Linux pen tablets such as the Framework 12.

## Features

- **One page at a time.** The page fits the whole window. Turn pages with the toolbar,
  Alt+Page Up/Down, or a quick two-finger swipe.
- **The MarpX toolbar.** Dark, compact, dockable on any window edge. Only the tools you
  need for teaching:
  - pen and highlighter, with 8 colors and 4 sizes each
  - selector: tap, rectangle, or lasso, depending on how you draw
  - laser pointer with a fading comet trail
  - eraser (whole strokes) and partial eraser, with a round cursor
  - undo, redo, clear page
- **Blank pages.** Insert a plain, lined, or grid page after the current slide (Ctrl+Shift+B),
  for example for a worked example. Blank pages can be deleted again. The counter shows
  them as "3 / 37 · blank 1/2".
- **Your PDF stays untouched.** Notes are saved as `.rnote` files and exported as an
  annotated PDF.
- **Rnote compatible.** Tafel reads and writes the Rnote file format, so the notes also open in
  Rnote.

## What changed compared to Rnote

| | Rnote | Tafel |
|---|---|---|
| Toolbar | Sidebars, pen picker, color picker | One MarpX-style bar |
| Tools | Brush styles, shapes, typewriter, tools | Pen, highlighter, selector, laser, two erasers |
| Page view | Continuous or infinite canvas | Single page, fitted to the window |
| Navigation | Scrolling | Page turns, keyboard, two-finger swipe |
| Blank pages | Append at the end | Insert after any page, with paper |
| PDF import | Asks for spacing and width | One page per PDF page, full width |
| App ID | `com.github.flxzt.rnote` | `io.github.ansgomez.tafel` |

Tafel and Rnote can be installed side by side; they use separate settings.

## Install from source

Tested on Fedora 44. Install the build dependencies:

```bash
sudo dnf install \
    gcc gcc-c++ clang clang-devel python3 make cmake meson git appstream gettext \
    desktop-file-utils shared-mime-info gtk4-devel libadwaita-devel alsa-lib-devel \
    appstream-devel
```

Make sure `rustc` and `cargo` are installed (see [rustup.rs](https://rustup.rs)). Then:

```bash
git clone https://github.com/ansgomez/tafel
cd tafel
meson setup --prefix="$HOME/.local" _mesonbuild
meson compile -C _mesonbuild
meson install -C _mesonbuild
```

Start it from the app launcher or with `tafel some-slides.pdf`.

The full build takes several minutes. For faster local rebuilds, you can add a
`.cargo/config.toml` with a lighter release profile, for example `lto = "thin"` and
`codegen-units = 16`.

More build options (Flatpak, Windows) are described in [BUILDING.md](BUILDING.md), which still
uses the upstream Rnote names.

## Usage

| Action | Input |
|---|---|
| Next / previous page | Toolbar arrows, Alt+Page Down / Alt+Page Up, two-finger swipe left / right |
| Insert a blank page after the current one | Toolbar, Ctrl+Shift+B |
| Undo / redo | Toolbar, Ctrl+Z / Ctrl+Shift+Z |
| Move the toolbar to another edge | First toolbar button |
| Zoom | Pinch; the next page turn fits the page again |

## Roadmap

- Pen-hover laser and a slide overview, as in MarpX.
- A cross-platform successor (Linux, Windows, Android, iPad) built with Tauri 2, PDF.js and
  perfect-freehand, reusing the MarpX annotation code. That will be a separate MIT-licensed
  project without Rnote code. See [docs/tafel-next-devplan.md](docs/tafel-next-devplan.md).

## License and credits

Tafel is licensed under the **GNU General Public License v3.0 or later**, the same license
as Rnote. See [LICENSE](LICENSE).

- **[Rnote](https://github.com/flxzt/rnote)**: © Felix Zwettler and the Rnote contributors
  (see [AUTHORS](AUTHORS)), GPL-3.0-or-later. The crates in `crates/` keep their upstream
  names (`rnote-engine`, `rnote-compose`, …), so upstream changes can still be merged.
- **Tafel changes**: © 2026 Andres Gomez, GPL-3.0-or-later. The commit history shows every
  change on top of Rnote.
- **[MarpX](https://github.com/cunhapaulo/MarpX)** by Paulo Cunha (MIT) inspired the look.
  The toolbar design follows the MarpX annotation mode written by Andres Gomez.
