#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Record the animated README demo of Tafel without touching the desktop session.

How it works:
- Tafel runs on GTK's Broadway backend (`gtk4-broadwayd`), which renders the app into a web page.
- A headless Firefox with its own throwaway profile opens that page. The script drives it over
  Firefox's built-in Marionette (WebDriver) protocol: it moves the mouse to draw strokes and
  takes a screenshot after each step.
- Pillow assembles the screenshots into an animated WebP.

Settings use GLib's keyfile backend and all XDG directories point into a temporary directory,
so the user's Tafel settings and recent files are left alone.

Requirements: gtk4-broadwayd (part of gtk4), firefox, python3 with pycairo and Pillow,
glib-compile-schemas, and a built Tafel (meson compile -C _mesonbuild).

Usage (from the repository root):
    python3 misc/demo/record_demo.py [--tafel _mesonbuild/target/release/tafel]
                                     [--out misc/media/tafel-demo.webp]
"""

import argparse
import base64
import io
import json
import math
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from PIL import Image, ImageDraw

REPO = Path(__file__).resolve().parents[2]
WIDTH, HEIGHT = 1280, 800
BROADWAY_DISPLAY = 7
BROADWAY_PORT = 8080 + BROADWAY_DISPLAY
MARIONETTE_PORT = 28287


class Marionette:
    """Minimal client for Firefox's Marionette protocol (length-prefixed JSON messages)."""

    def __init__(self, port):
        deadline = time.time() + 30
        while True:
            try:
                self.sock = socket.create_connection(("127.0.0.1", port))
                break
            except OSError:
                if time.time() > deadline:
                    raise
                time.sleep(0.3)
        self.msg_id = 0
        self._recv()  # handshake
        self.call("WebDriver:NewSession", {"capabilities": {}})

    def _recv(self):
        length = b""
        while not length.endswith(b":"):
            length += self.sock.recv(1)
        size = int(length[:-1])
        data = b""
        while len(data) < size:
            data += self.sock.recv(size - len(data))
        return json.loads(data)

    def call(self, command, params=None):
        self.msg_id += 1
        payload = json.dumps([0, self.msg_id, command, params or {}]).encode()
        self.sock.sendall(str(len(payload)).encode() + b":" + payload)
        _, _, error, result = self._recv()
        if error:
            raise RuntimeError(f"{command}: {error}")
        return result

    def screenshot(self):
        data = self.call("WebDriver:TakeScreenshot", {"full": False})["value"]
        return Image.open(io.BytesIO(base64.b64decode(data))).convert("RGB")

    def pointer(self, actions):
        self.call(
            "WebDriver:PerformActions",
            {
                "actions": [
                    {
                        "type": "pointer",
                        "id": "pen",
                        "parameters": {"pointerType": "mouse"},
                        "actions": actions,
                    }
                ]
            },
        )


class Recorder:
    """Collects screenshots and draws a pointer on them (headless screenshots show no cursor)."""

    def __init__(self, browser):
        self.browser = browser
        self.frames = []
        self.durations = []
        self.cursor = None

    def snap(self, duration_ms=60):
        frame = self.browser.screenshot()
        if self.cursor:
            draw = ImageDraw.Draw(frame)
            x, y = self.cursor
            draw.ellipse((x - 7, y - 7, x + 7, y + 7), outline=(255, 255, 255), width=3)
            draw.ellipse((x - 5, y - 5, x + 5, y + 5), outline=(40, 40, 40), width=2)
        self.frames.append(frame)
        self.durations.append(duration_ms)

    def click(self, x, y, pause=0.4):
        self.cursor = (x, y)
        click(self.browser, x, y)
        time.sleep(pause)
        self.snap(250)

    def hold(self, duration_ms):
        self.snap(duration_ms)

    def stroke(self, points, points_per_frame=4):
        """Draw a polyline with the primary mouse button held, recording frames on the way."""
        x, y = points[0]
        self.cursor = (x, y)
        self.browser.pointer(
            [
                {"type": "pointerMove", "x": int(x), "y": int(y), "duration": 0},
                {"type": "pointerDown", "button": 0},
            ]
        )
        for i in range(1, len(points), points_per_frame):
            chunk = points[i : i + points_per_frame]
            self.browser.pointer(
                [
                    {"type": "pointerMove", "x": int(px), "y": int(py), "duration": 15}
                    for px, py in chunk
                ]
            )
            self.cursor = chunk[-1]
            self.snap(50)
        self.browser.pointer([{"type": "pointerUp", "button": 0}])
        self.browser.call("WebDriver:ReleaseActions")


def lerp_path(a, b, n=12):
    return [(a[0] + (b[0] - a[0]) * t / n, a[1] + (b[1] - a[1]) * t / n) for t in range(n + 1)]


def ellipse(cx, cy, rx, ry, n=40, start=-1.9):
    return [
        (cx + rx * math.cos(start + 2.15 * math.pi * t / n), cy + ry * math.sin(start + 2.15 * math.pi * t / n))
        for t in range(n + 1)
    ]


def wavy_underline(x0, x1, y, n=24):
    return [(x0 + (x1 - x0) * t / n, y + 3 * math.sin(t * 1.1)) for t in range(n + 1)]


def compile_schema(tmp):
    schema_dir = tmp / "schemas"
    schema_dir.mkdir()
    src = REPO / "_mesonbuild/crates/rnote-ui/data/io.github.ansgomez.tafel.gschema.xml"
    if not src.exists():
        sys.exit(f"{src} not found. Run `meson setup` first.")
    shutil.copy(src, schema_dir)
    subprocess.run(["glib-compile-schemas", str(schema_dir)], check=True)
    return schema_dir


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--tafel", default=str(REPO / "_mesonbuild/target/release/tafel"))
    parser.add_argument("--out", default=str(REPO / "misc/media/tafel-demo.webp"))
    parser.add_argument("--keep-frames", action="store_true", help="also save the raw PNG frames")
    args = parser.parse_args()

    tmp = Path(tempfile.mkdtemp(prefix="tafel-demo-"))
    procs = []
    try:
        pdf = tmp / "lecture.pdf"
        subprocess.run([sys.executable, str(REPO / "misc/demo/make_sample_pdf.py"), str(pdf)], check=True)

        env = dict(os.environ)
        for var in ("WAYLAND_DISPLAY", "DISPLAY"):
            env.pop(var, None)
        env.update(
            GDK_BACKEND="broadway",
            BROADWAY_DISPLAY=f":{BROADWAY_DISPLAY}",
            GSETTINGS_BACKEND="keyfile",
            GSETTINGS_SCHEMA_DIR=str(compile_schema(tmp)),
            XDG_CONFIG_HOME=str(tmp / "config"),
            XDG_DATA_HOME=str(tmp / "data"),
            XDG_CACHE_HOME=str(tmp / "cache"),
            XDG_STATE_HOME=str(tmp / "state"),
            GTK_A11Y="none",
        )
        log = open(tmp / "log.txt", "w")
        procs.append(
            subprocess.Popen(
                ["gtk4-broadwayd", "-a", "127.0.0.1", "-p", str(BROADWAY_PORT), f":{BROADWAY_DISPLAY}"],
                env=env,
                stdout=log,
                stderr=log,
            )
        )
        time.sleep(1)

        profile = tmp / "firefox-profile"
        profile.mkdir()
        (profile / "user.js").write_text(
            f'user_pref("marionette.port", {MARIONETTE_PORT});\n'
            'user_pref("browser.shell.checkDefaultBrowser", false);\n'
            'user_pref("datareporting.policy.dataSubmissionEnabled", false);\n'
        )
        ff_env = dict(os.environ, MOZ_HEADLESS="1")
        procs.append(
            subprocess.Popen(
                ["firefox", "--headless", "--marionette", "--no-remote", "--profile", str(profile),
                 "--width", str(WIDTH), "--height", str(HEIGHT)],
                env=ff_env,
                stdout=log,
                stderr=log,
            )
        )
        browser = Marionette(MARIONETTE_PORT)
        browser.call("WebDriver:SetWindowRect", {"width": WIDTH, "height": HEIGHT})
        browser.call("WebDriver:Navigate", {"url": f"http://127.0.0.1:{BROADWAY_PORT}/"})
        time.sleep(1)

        viewport = browser.call("WebDriver:ExecuteScript", {"script": "return [innerWidth, innerHeight]", "args": []})["value"]
        keyfile = tmp / "config/glib-2.0/settings/keyfile"
        keyfile.parent.mkdir(parents=True)
        keyfile.write_text(
            "[io/github/ansgomez/tafel]\n"
            f"window-width={viewport[0]}\nwindow-height={viewport[1]}\nis-maximized=true\n"
            "color-scheme='force-dark'\n"
        )
        procs.append(subprocess.Popen([args.tafel, str(pdf)], env=env, stdout=log, stderr=log))
        time.sleep(6)

        rec = Recorder(browser)
        demo_script(browser, rec)

        if args.keep_frames:
            frames_dir = Path(args.out).with_suffix("")
            frames_dir.mkdir(parents=True, exist_ok=True)
            for i, frame in enumerate(rec.frames):
                frame.save(frames_dir / f"{i:04}.png")
        save_webp(rec.frames, rec.durations, Path(args.out))
    finally:
        for proc in reversed(procs):
            proc.terminate()
            try:
                proc.wait(5)
            except subprocess.TimeoutExpired:
                proc.kill()
        shutil.rmtree(tmp, ignore_errors=True)


def demo_script(browser, rec):
    """Confirm the PDF import, then annotate the first slide."""
    time.sleep(4)
    click_import_button(browser)
    time.sleep(4)
    rec.snap(1500)

    # Map slide coordinates (PDF points, 960 x 540) to the page shown in the browser.
    left, top, right, _ = page_bbox(browser.screenshot())
    scale = (right - left) / 960

    def slide(points):
        return [(left + x * scale, top + y * scale) for x, y in points]

    # Toolbar buttons, in viewport pixels for the 1280 px wide window.
    # Measured from a screenshot; update them if the toolbar layout changes.
    pen, red, blue, size_m = (394, 70), (698, 70), (735, 70), (1000, 70)
    next_page = (1244, 91)

    # Highlight the second bullet.
    # The highlighter is left out: Broadway cannot render its blend mode and shows magenta.

    # Circle the question and answer it.
    rec.click(*pen)
    rec.click(*red)
    rec.click(*size_m)
    rec.stroke(slide(ellipse(448, 364, 58, 24)))
    rec.hold(300)
    rec.click(*blue)
    for part in handwritten_8v(522, 362):
        rec.stroke(slide(part))
    rec.hold(300)

    # Underline the rule that answers it.
    rec.click(*red)
    rec.stroke(slide(wavy_underline(580, 760, 300)))
    rec.hold(2000)

    # Turn the page.
    rec.click(*next_page, pause=1.5)
    rec.hold(2500)


def handwritten_8v(x, y):
    """Strokes for a handwritten "= 8 V" starting at (x, y), the middle of the text line."""
    turn = [2 * math.pi * t / 30 for t in range(31)]
    eight = [(x + 32 + 6 * math.sin(a), y - 7 - 6 * math.cos(a)) for a in turn]
    eight += [(x + 32 - 7 * math.sin(a), y + 6 - 7 * math.cos(a)) for a in turn]
    return [
        lerp_path((x, y - 4), (x + 14, y - 4), 4),
        lerp_path((x, y + 4), (x + 14, y + 4), 4),
        eight,
        lerp_path((x + 50, y - 13), (x + 58, y + 13), 8) + lerp_path((x + 58, y + 13), (x + 66, y - 13), 8)[1:],
    ]


def page_bbox(image):
    """Bounding box of the white page in a screenshot."""
    mask = image.convert("L").point(lambda v: 255 if v > 250 else 0)
    mask.paste(0, (0, 0, image.width, 140))  # ignore the toolbar (it has a white color button)
    return mask.getbbox()


def find_color(image, rgb, tolerance=12, min_pixels=200):
    """Return the center of the pixels close to `rgb`, or None."""
    xs, ys = [], []
    px = image.load()
    for y in range(0, image.height, 2):
        for x in range(0, image.width, 2):
            p = px[x, y]
            if all(abs(p[i] - rgb[i]) <= tolerance for i in range(3)):
                xs.append(x)
                ys.append(y)
    if len(xs) * 4 < min_pixels:
        return None
    return sum(xs) // len(xs), sum(ys) // len(ys)


def click(browser, x, y):
    browser.pointer(
        [
            {"type": "pointerMove", "x": int(x), "y": int(y), "duration": 0},
            {"type": "pointerDown", "button": 0},
            {"type": "pointerUp", "button": 0},
        ]
    )
    browser.call("WebDriver:ReleaseActions")


def click_import_button(browser):
    """Click the blue (suggested action) Import button in the header of the PDF import dialog."""
    for _ in range(20):
        shot = browser.screenshot()
        pos = find_color(shot.crop((0, 0, shot.width, 60)), (53, 132, 228), tolerance=25)
        if pos:
            click(browser, *pos)
            return
        time.sleep(1)
    raise RuntimeError("PDF import dialog not found")


def save_webp(frames, durations, out):
    out.parent.mkdir(parents=True, exist_ok=True)
    width = 960
    size = (width, round(frames[0].height * width / frames[0].width))
    small = [f.resize(size, Image.LANCZOS) for f in frames]
    small[0].save(
        out,
        save_all=True,
        append_images=small[1:],
        duration=durations,
        loop=0,
        quality=80,
        method=6,
    )
    print(f"wrote {out} ({out.stat().st_size // 1024} KiB, {len(frames)} frames)")


if __name__ == "__main__":
    main()
