#!/usr/bin/env python3
"""Estimate the translation that best aligns each GPUI component to Electron."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter


REGIONS = {
    "sidebar_nav": (8, 82, 250, 246),
    "sidebar_projects": (8, 250, 250, 795),
    "home_heading": (600, 330, 1100, 455),
    "suggestion_1": (500, 575, 910, 620),
    "suggestion_2": (500, 620, 930, 665),
    "referral": (480, 675, 1220, 738),
    "utility": (480, 742, 1220, 786),
    "composer_footer": (480, 830, 1220, 884),
    "theme_switch": (1295, 825, 1428, 890),
}


def edge_image(path: Path) -> np.ndarray:
    image = Image.open(path).convert("L").filter(ImageFilter.FIND_EDGES)
    return np.asarray(image, dtype=np.float32)


def alignment(reference: np.ndarray, actual: np.ndarray, bounds, radius: int) -> dict:
    left, top, right, bottom = bounds
    best = None
    for dy in range(-radius, radius + 1):
        for dx in range(-radius, radius + 1):
            # Moving the actual image by (dx, dy) means sampling its old pixels
            # at (x - dx, y - dy).
            r_left = left + max(0, dx)
            r_right = right + min(0, dx)
            r_top = top + max(0, dy)
            r_bottom = bottom + min(0, dy)
            ref_crop = reference[r_top:r_bottom, r_left:r_right]
            got_crop = actual[
                r_top - dy:r_bottom - dy,
                r_left - dx:r_right - dx,
            ]
            error = float(np.mean(np.abs(ref_crop - got_crop)))
            candidate = (error, abs(dx) + abs(dy), dx, dy)
            if best is None or candidate < best:
                best = candidate
    assert best is not None
    error, _, dx, dy = best
    return {"move_actual_x": dx, "move_actual_y": dy, "edge_mae": round(error, 6)}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("reference", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument("--radius", type=int, default=12)
    args = parser.parse_args()
    reference = edge_image(args.reference)
    actual = edge_image(args.actual)
    if reference.shape != actual.shape:
        raise SystemExit(f"size mismatch: {reference.shape} != {actual.shape}")
    report = {
        name: alignment(reference, actual, bounds, args.radius)
        for name, bounds in REGIONS.items()
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
