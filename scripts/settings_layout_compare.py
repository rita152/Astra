#!/usr/bin/env python3
"""Compare GPUI settings captures with Electron while tolerating rasterizer noise."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageChops, ImageEnhance, ImageFilter


def edge_mask(image: Image.Image, threshold: int) -> Image.Image:
    edges = image.convert("L").filter(ImageFilter.FIND_EDGES)
    return edges.point(lambda value: 255 if value >= threshold else 0)


def count_mask(image: Image.Image) -> int:
    return sum(value > 0 for value in image.get_flattened_data())


def covered(source: Image.Image, target_dilated: Image.Image) -> int:
    return sum(
        source_value > 0 and target_value > 0
        for source_value, target_value in zip(
            source.get_flattened_data(), target_dilated.get_flattened_data()
        )
    )


def structural_segments(mask: Image.Image, minimum_length: int = 36) -> list[tuple[str, int, int, int]]:
    width, height = mask.size
    values = list(mask.get_flattened_data())
    segments: list[tuple[str, int, int, int]] = []

    def runs(bits: list[bool]) -> list[tuple[int, int]]:
        result = []
        start = None
        for index, active in enumerate(bits + [False]):
            if active and start is None:
                start = index
            elif not active and start is not None:
                if index - start >= minimum_length:
                    result.append((start, index - 1))
                start = None
        return result

    for y in range(height):
        for start, end in runs([values[y * width + x] > 0 for x in range(width)]):
            segments.append(("h", y, start, end))
    for x in range(width):
        for start, end in runs([values[y * width + x] > 0 for y in range(height)]):
            segments.append(("v", x, start, end))
    return segments


def segment_match(source: tuple[str, int, int, int], targets: list[tuple[str, int, int, int]]) -> bool:
    orientation, axis, start, end = source
    length = end - start + 1
    for target_orientation, target_axis, target_start, target_end in targets:
        if orientation != target_orientation or abs(axis - target_axis) > 2:
            continue
        overlap = max(0, min(end, target_end) - max(start, target_start) + 1)
        if overlap / max(length, target_end - target_start + 1) >= 0.9:
            return True
    return False


def compare(reference: Path, actual: Path, output: Path, tolerance: int) -> dict[str, object]:
    ref = Image.open(reference).convert("RGB")
    got = Image.open(actual).convert("RGB")
    if ref.size != got.size:
        raise ValueError(f"size mismatch: {ref.size} != {got.size}")

    diff = ImageChops.difference(ref, got)
    pixels = list(diff.get_flattened_data())
    total = len(pixels)
    within_tolerance = sum(max(pixel) <= tolerance for pixel in pixels)
    normalized = 1 - sum(sum(pixel) for pixel in pixels) / (total * 255 * 3)

    # A one-pixel bidirectional edge allowance absorbs Skia/CoreText versus
    # GPUI rasterization without accepting shifted boxes, dividers, or cards.
    ref_edges = edge_mask(ref, 18)
    got_edges = edge_mask(got, 18)
    ref_total = count_mask(ref_edges)
    got_total = count_mask(got_edges)
    ref_dilated = ref_edges.filter(ImageFilter.MaxFilter(3))
    got_dilated = got_edges.filter(ImageFilter.MaxFilter(3))
    recall = covered(ref_edges, got_dilated) / max(ref_total, 1)
    precision = covered(got_edges, ref_dilated) / max(got_total, 1)
    layout_f1 = 2 * precision * recall / max(precision + recall, 1e-9)

    ref_segments = structural_segments(ref_edges)
    got_segments = structural_segments(got_edges)
    structural_recall = sum(segment_match(item, got_segments) for item in ref_segments) / max(
        len(ref_segments), 1
    )
    structural_precision = sum(segment_match(item, ref_segments) for item in got_segments) / max(
        len(got_segments), 1
    )
    structural_f1 = 2 * structural_precision * structural_recall / max(
        structural_precision + structural_recall, 1e-9
    )

    # Blurring by less than one pixel suppresses subpixel glyph coverage while
    # retaining one-pixel translations and all component geometry changes.
    ref_soft = ref.filter(ImageFilter.GaussianBlur(0.65))
    got_soft = got.filter(ImageFilter.GaussianBlur(0.65))
    soft_pixels = list(ImageChops.difference(ref_soft, got_soft).get_flattened_data())
    soft_consistency = sum(max(pixel) <= tolerance for pixel in soft_pixels) / total

    # Layout acceptance uses luminance blurred by two pixels. This suppresses
    # CoreText/Skia glyph coverage and native shadow noise while retaining card
    # bounds, spacing, columns, dividers, and controls at screenshot scale.
    ref_layout = ref.convert("L").filter(ImageFilter.GaussianBlur(2.0))
    got_layout = got.convert("L").filter(ImageFilter.GaussianBlur(2.0))
    layout_diff = ImageChops.difference(ref_layout, got_layout)
    layout_values = list(layout_diff.get_flattened_data())
    layout_similarity = 1 - sum(layout_values) / (total * 255)

    output.mkdir(parents=True, exist_ok=True)
    ImageEnhance.Contrast(diff).enhance(4).save(output / "diff.png")
    Image.blend(ref, got, 0.5).save(output / "overlay.png")
    return {
        "reference": str(reference),
        "actual": str(actual),
        "pixel_consistency": round(within_tolerance / total * 100, 6),
        "soft_pixel_consistency": round(soft_consistency * 100, 6),
        # Keep the two acceptance metrics unrounded so a value microscopically
        # below 99 cannot be rounded up and incorrectly pass the hard gate.
        "layout_similarity": layout_similarity * 100,
        "normalized_similarity": normalized * 100,
        "layout_edge_precision": round(precision * 100, 6),
        "layout_edge_recall": round(recall * 100, 6),
        "layout_edge_f1": round(layout_f1 * 100, 6),
        "structural_precision": round(structural_precision * 100, 6),
        "structural_recall": round(structural_recall * 100, 6),
        "structural_f1": round(structural_f1 * 100, 6),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("reference", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--tolerance", type=int, default=12)
    parser.add_argument("--min-soft-consistency", type=float, default=85.0)
    parser.add_argument("--min-normalized-similarity", type=float, default=99.5)
    parser.add_argument("--min-layout-similarity", type=float, default=99.0)
    parser.add_argument("--min-layout-edge-f1", type=float, default=45.0)
    args = parser.parse_args()

    if not 0 <= args.tolerance <= 255:
        parser.error("--tolerance must be between 0 and 255")
    for name in (
        "min_soft_consistency",
        "min_normalized_similarity",
        "min_layout_similarity",
        "min_layout_edge_f1",
    ):
        if not 0 <= getattr(args, name) <= 100:
            parser.error(f"--{name.replace('_', '-')} must be between 0 and 100")

    report = compare(args.reference, args.actual, args.output, args.tolerance)
    thresholds = {
        "min_soft_pixel_consistency": args.min_soft_consistency,
        "min_normalized_similarity": args.min_normalized_similarity,
        "min_layout_similarity": args.min_layout_similarity,
        "min_layout_edge_f1": args.min_layout_edge_f1,
    }
    checks = (
        ("soft_pixel_consistency", args.min_soft_consistency),
        ("normalized_similarity", args.min_normalized_similarity),
        ("layout_similarity", args.min_layout_similarity),
        ("layout_edge_f1", args.min_layout_edge_f1),
    )
    report["thresholds"] = thresholds
    report["failed_thresholds"] = [
        f"{metric}={report[metric]:.12f} < {minimum:.12f}"
        for metric, minimum in checks
        if report[metric] < minimum
    ]
    report["passing"] = not report["failed_thresholds"]
    (args.output / "report.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    )
    print(json.dumps(report, ensure_ascii=False))
    if not report["passing"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
