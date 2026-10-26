#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Generate a small lecture-style sample PDF for the Tafel demo.

The slides are drawn from scratch with pycairo, so the PDF carries no third-party content.

Usage: python3 make_sample_pdf.py OUTPUT.pdf
"""

import math
import sys

import cairo

W, H = 960, 540  # 16:9 slide in PDF points

ACCENT = (0.13, 0.36, 0.62)
TEXT = (0.15, 0.15, 0.18)


def header(ctx, title, number):
    ctx.set_source_rgb(*ACCENT)
    ctx.rectangle(0, 0, W, 8)
    ctx.fill()
    ctx.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
    ctx.set_font_size(38)
    ctx.move_to(56, 82)
    ctx.show_text(title)
    ctx.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
    ctx.set_font_size(14)
    ctx.set_source_rgb(0.5, 0.5, 0.55)
    ctx.move_to(56, H - 28)
    ctx.show_text("Circuits 101 · Lecture 3")
    ctx.move_to(W - 76, H - 28)
    ctx.show_text(str(number))


def bullets(ctx, x, y, lines, size=24):
    ctx.set_source_rgb(*TEXT)
    ctx.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
    ctx.set_font_size(size)
    for line in lines:
        ctx.new_sub_path()
        ctx.arc(x, y - size * 0.32, 4, 0, 2 * math.pi)
        ctx.fill()
        ctx.move_to(x + 18, y)
        ctx.show_text(line)
        y += size * 1.8


def resistor(ctx, x, y0, y1, label):
    """Vertical zigzag resistor from y0 to y1 at x."""
    mid0, mid1 = y0 + (y1 - y0) * 0.25, y0 + (y1 - y0) * 0.75
    ctx.move_to(x, y0)
    ctx.line_to(x, mid0)
    steps = 6
    for i in range(1, steps + 1):
        yy = mid0 + (mid1 - mid0) * i / steps
        xx = x + (14 if i % 2 else -14) if i < steps else x
        ctx.line_to(xx, yy)
    ctx.line_to(x, y1)
    ctx.stroke()
    ctx.move_to(x + 24, (y0 + y1) / 2 + 8)
    ctx.show_text(label)


def divider(ctx, ox, oy):
    ctx.set_source_rgb(*TEXT)
    ctx.set_line_width(3)
    ctx.select_font_face("Sans", cairo.FONT_SLANT_ITALIC, cairo.FONT_WEIGHT_NORMAL)
    ctx.set_font_size(24)
    # Source on the left
    ctx.move_to(ox, oy)
    ctx.line_to(ox + 140, oy)
    ctx.move_to(ox, oy + 300)
    ctx.line_to(ox + 140, oy + 300)
    ctx.move_to(ox, oy)
    ctx.line_to(ox, oy + 120)
    ctx.move_to(ox, oy + 180)
    ctx.line_to(ox, oy + 300)
    ctx.stroke()
    ctx.new_sub_path()
    ctx.arc(ox, oy + 150, 30, 0, 2 * math.pi)
    ctx.stroke()
    ctx.move_to(ox - 9, oy + 142)
    ctx.show_text("+")
    ctx.move_to(ox - 70, oy + 158)
    ctx.show_text("U")
    resistor(ctx, ox + 140, oy, oy + 150, "R₁")
    resistor(ctx, ox + 140, oy + 150, oy + 300, "R₂")
    # Output terminals
    ctx.move_to(ox + 140, oy + 150)
    ctx.line_to(ox + 260, oy + 150)
    ctx.move_to(ox + 140, oy + 300)
    ctx.line_to(ox + 260, oy + 300)
    ctx.stroke()
    for ty in (oy + 150, oy + 300):
        ctx.new_sub_path()
        ctx.arc(ox + 266, ty, 6, 0, 2 * math.pi)
        ctx.stroke()
    ctx.move_to(ox + 290, oy + 232)
    ctx.show_text("U₂ = ?")


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "sample-lecture.pdf"
    surface = cairo.PDFSurface(out, W, H)
    surface.set_metadata(cairo.PDF_METADATA_TITLE, "Tafel demo slides")
    ctx = cairo.Context(surface)

    header(ctx, "The voltage divider", 1)
    divider(ctx, 120, 140)
    bullets(ctx, 560, 200, ["Same current I in R₁ and R₂", "U = U₁ + U₂", "U₁ / U₂ = R₁ / R₂"])
    ctx.show_page()

    header(ctx, "Worked example", 2)
    bullets(ctx, 80, 170, ["U = 12 V,  R₁ = 2 kΩ,  R₂ = 4 kΩ", "Find I and U₂"])
    ctx.show_page()

    surface.finish()


if __name__ == "__main__":
    main()
