#!/usr/bin/env python3
"""Deterministic pixel comparison with score, overlays and heatmaps."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageChops, ImageEnhance, ImageFilter


REGIONS = {
    "topbar": (0, 0, 1440, 50),
    "sidebar": (0, 46, 257, 900),
    "home_heading": (600, 315, 1100, 470),
    "suggestions": (470, 570, 930, 670),
    "referral": (470, 670, 1230, 742),
    "composer": (470, 742, 1230, 900),
    "theme_switch": (1290, 820, 1440, 900),
    "sidebar_header": (8, 46, 250, 84),
    "sidebar_nav": (8, 84, 250, 246),
    "sidebar_projects": (8, 246, 250, 814),
    "sidebar_account": (8, 854, 250, 900),
    "heading_icon": (815, 320, 882, 385),
    "heading_text": (625, 395, 1070, 445),
    "suggestion_icons": (500, 575, 530, 665),
    "suggestion_text": (528, 575, 930, 665),
    "referral_frame": (475, 672, 1222, 740),
    "referral_content": (488, 682, 1010, 730),
    "referral_buttons": (1070, 680, 1212, 730),
    "composer_utility": (480, 744, 1220, 786),
    "composer_frame": (475, 782, 1222, 890),
    "composer_footer_left": (484, 840, 620, 882),
    "composer_footer_right": (1025, 840, 1212, 882),
    "theme_frame": (1298, 826, 1428, 890),
}


def compare(reference: Path, actual: Path, output: Path, tolerance: int) -> dict[str, object]:
    ref = Image.open(reference).convert("RGBA")
    got = Image.open(actual).convert("RGBA")
    if ref.size != got.size:
        raise SystemExit(f"size mismatch: reference={ref.size}, actual={got.size}")

    output.mkdir(parents=True, exist_ok=True)
    diff = ImageChops.difference(ref, got)
    pixels = list(diff.get_flattened_data())
    total = len(pixels)
    matching = sum(max(pixel) <= tolerance for pixel in pixels)
    exact = sum(max(pixel) == 0 for pixel in pixels)
    absolute_error = sum(sum(pixel[:3]) for pixel in pixels)
    max_error = total * 255 * 3

    ref_edges = ref.convert("L").filter(ImageFilter.FIND_EDGES)
    got_edges = got.convert("L").filter(ImageFilter.FIND_EDGES)
    edge_mask = Image.frombytes(
        "L",
        ref.size,
        bytes(
            255 if max(a, b) > 12 else 0
            for a, b in zip(
                ref_edges.get_flattened_data(), got_edges.get_flattened_data()
            )
        ),
    ).filter(ImageFilter.MaxFilter(3))
    edge_flags = list(edge_mask.get_flattened_data())
    edge_total = sum(bool(value) for value in edge_flags)
    edge_matching = sum(
        bool(mask) and max(pixel) <= tolerance
        for mask, pixel in zip(edge_flags, pixels)
    )

    width, height = ref.size
    region_reports = {}
    for name, (left, top, right, bottom) in REGIONS.items():
        left, right = max(0, left), min(width, right)
        top, bottom = max(0, top), min(height, bottom)
        indices = [
            y * width + x
            for y in range(top, bottom)
            for x in range(left, right)
        ]
        region_total = len(indices)
        region_matching = sum(max(pixels[index]) <= tolerance for index in indices)
        region_edge_indices = [index for index in indices if edge_flags[index]]
        region_edge_matching = sum(
            max(pixels[index]) <= tolerance for index in region_edge_indices
        )
        region_reports[name] = {
            "pixel_consistency": round(region_matching / region_total * 100, 6),
            "different_pixels": region_total - region_matching,
            "edge_pixel_consistency": (
                round(region_edge_matching / len(region_edge_indices) * 100, 6)
                if region_edge_indices
                else 100.0
            ),
            "edge_pixels": len(region_edge_indices),
        }

    heat = ImageEnhance.Contrast(diff.convert("RGB")).enhance(4.0)
    overlay = Image.blend(ref.convert("RGB"), got.convert("RGB"), 0.5)
    heat.save(output / "diff.png")
    overlay.save(output / "overlay.png")

    report = {
        "reference": str(reference),
        "actual": str(actual),
        "size": list(ref.size),
        "tolerance": tolerance,
        "pixel_consistency": round(matching / total * 100, 6),
        "exact_pixel_consistency": round(exact / total * 100, 6),
        "normalized_similarity": round((1 - absolute_error / max_error) * 100, 6),
        "edge_pixel_consistency": round(edge_matching / edge_total * 100, 6),
        "edge_pixels": edge_total,
        "different_pixels": total - matching,
        "total_pixels": total,
        "regions": region_reports,
    }
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    return report


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("reference", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument("--output", type=Path, default=Path("artifacts/pixel-diff"))
    parser.add_argument("--tolerance", type=int, default=0)
    parser.add_argument("--min-consistency", type=float, default=100.0)
    parser.add_argument("--min-edge-consistency", type=float, default=100.0)
    args = parser.parse_args()

    report = compare(args.reference, args.actual, args.output, args.tolerance)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    if (
        report["pixel_consistency"] < args.min_consistency
        or report["edge_pixel_consistency"] < args.min_edge_consistency
    ):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
