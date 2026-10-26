# Building Tafel

> Tafel is a modified fork of [Rnote](https://github.com/flxzt/rnote).
> This file is adapted from Rnote's [BUILDING.md](https://github.com/flxzt/rnote/blob/main/BUILDING.md).

Tafel keeps Rnote's code layout. The crates keep their upstream names (`rnote`, `rnote-engine`, `rnote-cli`, ...).
The app itself is renamed:

| | Value |
|---|---|
| Binary | `tafel` (built from the `rnote` package in `crates/rnote-ui`) |
| App ID | `io.github.ansgomez.tafel` (`io.github.ansgomez.tafel.Devel` with the devel profile) |
| Resource path | `/io/github/ansgomez/tafel/` |
| CLI binary | `rnote-cli` (unchanged) |

The supported and tested path is a **native meson build on Fedora 44**.

# Get the source

```bash
sudo dnf install git
git clone https://github.com/ansgomez/tafel
cd tafel
```

The repository has no git submodules, so no `git submodule` step is needed.

# Build with meson

## Prerequisites

Install the build tools and libraries. For Fedora:

```bash
sudo dnf install \
    gcc gcc-c++ clang clang-devel python3 make cmake meson git appstream gettext desktop-file-utils \
    shared-mime-info gtk4-devel libadwaita-devel alsa-lib-devel appstream-devel
```

For Debian based distros (not tested for Tafel):

```bash
sudo apt install \
    build-essential clang libclang-dev python3 make cmake meson just git appstream gettext desktop-file-utils \
    shared-mime-info libgtk-4-dev libadwaita-1-dev libasound2-dev libappstream-dev
```

Install Rust either from your distribution (`sudo dnf install rust cargo` on Fedora) or with [rustup](https://rustup.rs/).
The minimum Rust version is set by `rust-version` in the root `Cargo.toml` (currently 1.89).

## Set up, compile and install

Install to your user prefix. This needs no root access:

```bash
meson setup --prefix="$HOME/.local" -Dcli=false _mesonbuild
meson compile -C _mesonbuild
meson install -C _mesonbuild
```

`-Dcli=false` skips `rnote-cli` and the `.rnote` thumbnailer. Leave it out if you want them, but
they then replace the copies of an Rnote install in the same prefix.

After install, Tafel shows up in the app menu under **Education**.
The desktop entry uses an absolute `Exec` path (`<prefix>/bin/tafel`),
because `~/.local/bin` is often not in the GNOME session `PATH`.

To install system-wide instead, use the `/usr` prefix and run the install step with `sudo`:

```bash
meson setup --prefix=/usr -Dcli=false _mesonbuild
meson compile -C _mesonbuild
sudo meson install -C _mesonbuild
```

The compiled binary is at `_mesonbuild/target/release/tafel`
(or `_mesonbuild/target/debug/tafel` with the devel profile).

## Configure

- Set `-Dprofile=devel` to use the development profile (debug build, app ID `io.github.ansgomez.tafel.Devel`).
  The default profile is `default`.
- The `rnote-cli` binary is built by default. Set `-Dcli=false` to skip it.

Change options on an existing build directory with:

```bash
meson configure -D<option>=<value> _mesonbuild
```

## Faster local builds (optional)

Release builds use full LTO and are slow. For local work you can override the release profile
in an untracked `.cargo/config.toml` in the repository root:

```toml
[profile.release]
lto = "thin"
codegen-units = 16
incremental = true
debug = "line-tables-only"
```

On a Framework 12 this gives about 6 minutes for a full rebuild and 2 to 2.5 minutes for an incremental one.
Keep `.cargo/` untracked. Do not commit it.

## Settings

Settings are stored under the gsettings schema `io.github.ansgomez.tafel`.

List them:

```bash
gsettings list-recursively io.github.ansgomez.tafel
```

Reset them to defaults:

```bash
gsettings reset-recursively io.github.ansgomez.tafel
```

If you see `Settings schema 'io.github.ansgomez.tafel' does not contain a key named '...'`,
the installed schema is older than your build. Run `meson install -C _mesonbuild` again to update it.

## Test

Run the meson tests. They validate the desktop, metainfo, gresource and similar files:

```bash
meson test -v -C _mesonbuild
```

Run the Rust unit and integration tests (needs `cargo-nextest`, install with `cargo install cargo-nextest`):

```bash
meson compile cargo-test -C _mesonbuild
```

## Custom targets

Run a custom target with:

```bash
meson compile <custom target> -C _mesonbuild
```

| target | explanation |
|---|---|
| tafel-pot | Regenerate the po template file. Provided by the `i18n` module. |
| tafel-update-po | Update the po files from the template. Provided by the `i18n` module. |
| tafel-gmo | Compile the po files. Provided by the `i18n` module. |
| cargo-fmt-check | Check the code formatting |
| cargo-test | Run all unit and integration tests (needs `cargo-nextest`) |
| cargo-clean | Clean artifacts that cargo has generated |
| ui-cargo-check | Run cargo check for the ui package |
| ui-cargo-clippy | Run clippy for the ui package |
| ui-cargo-doc | Generate docs for the ui package |
| ui-cargo-build | Build the ui |
| cli-cargo-check | Run cargo check for the cli package |
| cli-cargo-clippy | Run clippy for the cli package |
| cli-cargo-doc | Generate docs for the cli package |
| cli-cargo-build | Build the cli |

## Uninstall

```bash
ninja -C _mesonbuild uninstall
```

For a `/usr` install, run it with `sudo -E`.
Uninstalling does not remove your settings. Reset them first with `gsettings reset-recursively io.github.ansgomez.tafel` if you want a clean slate.

# Debugging

Configure meson with `-Dprofile=devel` to get a build with debug symbols.
Then set up, compile and install as above.

With VSCode, the `CodeLLDB` extension can debug the app from within the editor.
Open the repository as a workspace and pick "debug ui" in the "Run and Debug" panel.

# Flatpak, Windows, just recipes and nightlies

These build paths are inherited from Rnote and are **not tested for Tafel**:

- Flatpak manifests: `build-aux/io.github.ansgomez.tafel.Devel.json` and `.yaml` (renamed, but never built)
- Windows installer: Inno Setup script `build-aux/rnote_inno.iss.in`, built with MSYS2 (`build-installer` target)
- macOS: [docs/build-macos.md](docs/build-macos.md); Windows: [docs/build-win.md](docs/build-win.md) (upstream instructions)
- The `justfile` and the CI workflows in `.github/workflows`

Tafel only supports Linux. See upstream's [BUILDING.md](https://github.com/flxzt/rnote/blob/main/BUILDING.md) for how these paths work in Rnote.

# Recording the README demo

`misc/media/tafel-demo.webp` is recorded by a script, without touching your desktop session.
The script runs Tafel on GTK's Broadway backend (`gtk4-broadwayd`), opens the result in a headless Firefox
with a throwaway profile, draws on a generated sample slide through Firefox's Marionette protocol and saves
the screenshots as an animated WebP. Settings and all XDG directories point into a temporary directory.

Requirements: `gtk4-broadwayd` (from `gtk4`), `firefox`, and `python3` with `pycairo` and `Pillow`
(`sudo dnf install python3-cairo python3-pillow`). Build Tafel first, then run from the repository root:

```bash
python3 misc/demo/record_demo.py
```

The toolbar button positions in `misc/demo/record_demo.py` are measured for a 1280 px wide window.
Update them if the toolbar layout changes.

# Keeping up with upstream

The `upstream` remote points to `flxzt/rnote`. To pull in upstream changes:

```bash
git fetch upstream
git merge upstream/main
```

Expect merge conflicts in `crates/rnote-ui`, where most of Tafel's changes live.
