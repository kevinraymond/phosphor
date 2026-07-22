#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow"]
# ///
"""Locate Fosfora's live canvas on screen by looking for the pixels that move.

Window managers lie about geometry in ways that are tedious to chase: `xdotool search` returns
both the WM frame and the client, reparenting shifts the origin, and asking xdotool to resize
the client can make the WM resize the frame instead. Chasing those cases produced a capture
region that was consistently a few tens of pixels off, filming a border of desktop.

So don't ask — the canvas is the thing that is animating. But note *how* that is used here.

An earlier version took the bounding box of everything that changed, and that is not robust: a
terminal repainting just outside the window, or a notification, drags a corner of the box out
and shifts the origin. Two runs of the same capture disagreed by 45 px that way, and the second
filmed a strip of title bar into all 38 clips.

Instead, the caller passes the canvas SIZE (which the client window reports reliably) and this
searches for the position of that fixed-size window containing the most motion. Stray motion
outside the canvas cannot move the answer unless it is denser than the canvas itself, which
for a running visualiser it never is.

Prints "X Y W H" in absolute screen coordinates. Exits non-zero if nothing moved.

Usage:  find_canvas.py --region X Y W H [--size W H] [--display :0] [--gap 0.6]
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageChops


def grab(display: str, x: int, y: int, w: int, h: int, out: Path) -> None:
    subprocess.run(
        ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
         "-f", "x11grab", "-video_size", f"{w}x{h}", "-i", f"{display}+{x},{y}",
         "-frames:v", "1", str(out)],
        check=True,
    )


def motion_mask(display: str, region: tuple[int, int, int, int], gap: float,
                threshold: int, samples: int) -> Image.Image | None:
    """Union of per-pixel change across several grab pairs, as a 0/255 L-mode image."""
    x0, y0, w0, h0 = region
    acc = None
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        prev = tmp / "prev.png"
        grab(display, x0, y0, w0, h0, prev)
        for i in range(samples):
            cur = tmp / f"cur{i}.png"
            subprocess.run(["sleep", str(gap)], check=True)
            grab(display, x0, y0, w0, h0, cur)
            diff = ImageChops.difference(
                Image.open(prev).convert("RGB"), Image.open(cur).convert("RGB")
            ).convert("L").point(lambda p: 255 if p >= threshold else 0)
            acc = diff if acc is None else ImageChops.lighter(acc, diff)
            prev = cur
    return acc


def best_offset(mask: Image.Image, tw: int, th: int) -> tuple[int, int, int]:
    """Offset of the tw x th window covering the most motion. Returns (x, y, score)."""
    w, h = mask.size
    tw, th = min(tw, w), min(th, h)
    px = mask.load()

    # Integral image, so each candidate window is four lookups rather than tw*th.
    integral = [[0] * (w + 1) for _ in range(h + 1)]
    for y in range(h):
        row_sum = 0
        for x in range(w):
            row_sum += 1 if px[x, y] else 0
            integral[y + 1][x + 1] = integral[y][x + 1] + row_sum

    def window(x: int, y: int) -> int:
        return (integral[y + th][x + tw] - integral[y][x + tw]
                - integral[y + th][x] + integral[y][x])

    # Coarse-to-fine: a 4 px sweep then a local refine. Exhaustive at 1 px over a
    # 2000x1300 search area is ~2.6M windows in pure Python, which is needlessly slow.
    best = (0, 0, -1)
    for y in range(0, h - th + 1, 4):
        for x in range(0, w - tw + 1, 4):
            s = window(x, y)
            if s > best[2]:
                best = (x, y, s)
    bx, by, _ = best
    for y in range(max(0, by - 4), min(h - th, by + 4) + 1):
        for x in range(max(0, bx - 4), min(w - tw, bx + 4) + 1):
            s = window(x, y)
            if s > best[2]:
                best = (x, y, s)
    return best


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--region", nargs=4, type=int, required=True, metavar=("X", "Y", "W", "H"))
    ap.add_argument("--size", nargs=2, type=int, metavar=("W", "H"),
                    help="known canvas size; enables the sliding-window search")
    ap.add_argument("--display", default=":0")
    ap.add_argument("--gap", type=float, default=0.6, help="seconds between the two grabs")
    ap.add_argument("--threshold", type=int, default=8, help="per-channel delta counting as motion")
    ap.add_argument("--samples", type=int, default=3, help="grab pairs to union together")
    args = ap.parse_args()

    x0, y0, w0, h0 = args.region
    mask = motion_mask(args.display, (x0, y0, w0, h0), args.gap, args.threshold, args.samples)
    if mask is None or not mask.getbbox():
        print("no motion detected in region — is the app rendering?", file=sys.stderr)
        return 1

    if args.size:
        tw, th = args.size
        bx, by, score = best_offset(mask, tw, th)
        if score <= 0:
            print("no motion inside any candidate window", file=sys.stderr)
            return 1
        coverage = score / float(tw * th)
        print(f"canvas coverage {coverage:.1%}", file=sys.stderr)
        w, h = min(tw, mask.size[0]), min(th, mask.size[1])
    else:
        # Fallback: bounding box. Kept for ad-hoc use, but see the module docstring —
        # this is the mode that let stray desktop motion shift the origin.
        bx, by, right, bottom = mask.getbbox()
        w, h = right - bx, bottom - by

    w -= w % 2
    h -= h % 2
    print(x0 + bx, y0 + by, w, h)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
