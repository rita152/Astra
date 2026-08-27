#!/usr/bin/env python3
"""Validate and compare the 42 Electron/GPUI settings screenshot pairs."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageChops

from settings_layout_compare import compare


THEMES = ("light", "dark")
EXPECTED_SLUGS = (
    "general-settings",
    "import",
    "profile",
    "appearance",
    "voice",
    "agent",
    "personalization",
    "pets",
    "keyboard-shortcuts",
    "usage",
    "computer-use",
    "chronicle",
    "appshots",
    "plugins-settings",
    "browser-use",
    "hooks-settings",
    "connections",
    "git-settings",
    "local-environments",
    "worktrees",
    "data-controls",
)
EXPECTED_PANEL_COUNT = len(EXPECTED_SLUGS)
EXPECTED_SIZE = (1440, 900)
FOCUS_REGIONS = {"hooks-settings": (440, 50, 1260, 240)}
MIN_FOCUS_NORMALIZED_SIMILARITY = 99.0
PROJECT_ROOT = Path(__file__).resolve().parents[1]


def load_manifest_slugs(manifest_path: Path) -> list[str]:
    try:
        manifest = json.loads(manifest_path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"could not read settings manifest {manifest_path}: {error}") from error

    if not isinstance(manifest, list):
        raise SystemExit(f"settings manifest must be a JSON array: {manifest_path}")

    slugs = [
        item.get("slug")
        for item in manifest
        if isinstance(item, dict) and "slug" in item
    ]
    metadata = manifest[-1] if manifest and isinstance(manifest[-1], dict) else {}
    errors = []
    if tuple(slugs) != EXPECTED_SLUGS:
        errors.append(
            "manifest slugs must exactly match the canonical ordered 21-page settings set"
        )
    if any(not isinstance(slug, str) or not slug for slug in slugs):
        errors.append("every manifest slug must be a non-empty string")
    if len(set(slugs)) != len(slugs):
        errors.append("manifest slugs must be unique")
    if metadata.get("panelCount") != EXPECTED_PANEL_COUNT:
        errors.append(
            f"manifest panelCount must be {EXPECTED_PANEL_COUNT}, "
            f"found {metadata.get('panelCount')!r}"
        )
    if metadata.get("themes") != list(THEMES):
        errors.append(
            f"manifest themes must be {list(THEMES)!r}, found {metadata.get('themes')!r}"
        )
    if errors:
        raise SystemExit("invalid settings manifest:\n- " + "\n- ".join(errors))
    return slugs


def pngs_by_slug(directory: Path) -> dict[str, Path]:
    if not directory.is_dir():
        raise SystemExit(f"missing screenshot directory: {directory}")
    return {path.stem: path for path in directory.glob("*.png") if path.is_file()}


def validate_screenshot_sets(root: Path, slugs: list[str]) -> None:
    expected = set(slugs)
    errors = []
    for kind in ("reference", "actual"):
        for theme in THEMES:
            directory = root / kind / theme
            found = pngs_by_slug(directory)
            invalid_entries = sorted(
                path.name
                for path in directory.iterdir()
                if not path.is_file() or path.suffix != ".png"
            )
            missing = sorted(expected - found.keys())
            unexpected = sorted(found.keys() - expected)
            if missing:
                errors.append(f"{kind}/{theme} missing: {', '.join(missing)}")
            if unexpected:
                errors.append(f"{kind}/{theme} unexpected: {', '.join(unexpected)}")
            if invalid_entries:
                errors.append(
                    f"{kind}/{theme} contains non-canonical entries: "
                    + ", ".join(invalid_entries)
                )
            if len(found) != EXPECTED_PANEL_COUNT:
                errors.append(
                    f"{kind}/{theme} expected {EXPECTED_PANEL_COUNT} PNGs, found {len(found)}"
                )
            for slug, screenshot in found.items():
                try:
                    with Image.open(screenshot) as image:
                        if image.format != "PNG":
                            errors.append(f"{screenshot} is {image.format}, expected PNG")
                        if image.size != EXPECTED_SIZE:
                            errors.append(
                                f"{screenshot} is {image.size[0]}x{image.size[1]}, "
                                f"expected {EXPECTED_SIZE[0]}x{EXPECTED_SIZE[1]}"
                            )
                except OSError as error:
                    errors.append(f"could not read {kind}/{theme}/{slug}.png: {error}")
    if errors:
        raise SystemExit("invalid settings screenshot matrix:\n- " + "\n- ".join(errors))


def focus_normalized_similarity(
    reference: Path, actual: Path, box: tuple[int, int, int, int]
) -> float:
    with Image.open(reference) as reference_image, Image.open(actual) as actual_image:
        reference_crop = reference_image.convert("RGB").crop(box)
        actual_crop = actual_image.convert("RGB").crop(box)
    pixels = list(ImageChops.difference(reference_crop, actual_crop).get_flattened_data())
    return 100 * (1 - sum(sum(pixel) for pixel in pixels) / (len(pixels) * 3 * 255))


def threshold_failures(report: dict[str, object], args: argparse.Namespace) -> list[str]:
    checks = (
        ("normalized_similarity", args.min_normalized_similarity),
        ("layout_similarity", args.min_layout_similarity),
        ("soft_pixel_consistency", args.min_soft_consistency),
        ("layout_edge_f1", args.min_layout_edge_f1),
    )
    return [
        f"{metric}={report[metric]:.12f} < {minimum:.12f}"
        for metric, minimum in checks
        if report[metric] < minimum
    ]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("artifacts/settings-matrix"))
    parser.add_argument(
        "--manifest",
        type=Path,
        default=PROJECT_ROOT / "chat-reference/settings/manifest.json",
    )
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
    if args.min_normalized_similarity < 99.5:
        parser.error("--min-normalized-similarity cannot be lower than the 99.5% hard gate")
    if args.min_layout_similarity < 99.0:
        parser.error("--min-layout-similarity cannot be lower than the 99% hard gate")

    slugs = load_manifest_slugs(args.manifest)
    validate_screenshot_sets(args.root, slugs)

    reports = []
    for theme in THEMES:
        for slug in slugs:
            reference = args.root / "reference" / theme / f"{slug}.png"
            actual = args.root / "actual" / theme / reference.name
            output = args.root / "diff" / theme / slug
            report = compare(reference, actual, output, args.tolerance)
            report.update({"theme": theme, "slug": slug})
            report["failed_thresholds"] = threshold_failures(report, args)
            if slug in FOCUS_REGIONS:
                focus_box = FOCUS_REGIONS[slug]
                focus_similarity = focus_normalized_similarity(reference, actual, focus_box)
                report.update(
                    {
                        "focus_region": list(focus_box),
                        "focus_normalized_similarity": focus_similarity,
                    }
                )
                if focus_similarity < MIN_FOCUS_NORMALIZED_SIMILARITY:
                    report["failed_thresholds"].append(
                        "focus_normalized_similarity="
                        f"{focus_similarity:.12f} < {MIN_FOCUS_NORMALIZED_SIMILARITY:.12f}"
                    )
            report["passing"] = not report["failed_thresholds"]
            reports.append(report)

    failures = [report for report in reports if not report["passing"]]
    summary = {
        "acceptance": {
            "required_pairs": EXPECTED_PANEL_COUNT * len(THEMES),
            "expected_size": list(EXPECTED_SIZE),
            "themes": list(THEMES),
            "slugs": slugs,
            "pixel_similarity_definition": (
                "normalized_similarity = 100 * (1 - sum(abs(reference RGB - actual RGB)) "
                "/ (width * height * 3 * 255)); it is measured on unblurred 1440x900 "
                "Electron-reference and GPUI-actual pixels, and every page/theme pair must "
                "meet the configured threshold (99.5% by default)"
            ),
            "layout_similarity_definition": (
                "layout_similarity is the normalized absolute luminance similarity after "
                "a 2px Gaussian blur; every page/theme pair must meet 99.0% by default"
            ),
            "focus_similarity_definition": (
                "focus_normalized_similarity uses the same unblurred RGB formula inside "
                "a page-specific content crop; Hooks must independently meet 99.0% so "
                "empty background pixels cannot hide a broken card or header"
            ),
            "soft_pixel_consistency_definition": (
                "percentage of pixels whose maximum RGB-channel delta is within tolerance "
                "after a 0.65px Gaussian blur"
            ),
            "layout_edge_f1_definition": (
                "F1 score for thresholded luminance edges with a bidirectional 1px allowance"
            ),
        },
        "page_count": len(reports),
        "thresholds": {
            "tolerance": args.tolerance,
            "min_soft_pixel_consistency": args.min_soft_consistency,
            "min_normalized_similarity": args.min_normalized_similarity,
            "min_layout_similarity": args.min_layout_similarity,
            "min_focus_normalized_similarity": MIN_FOCUS_NORMALIZED_SIMILARITY,
            "min_layout_edge_f1": args.min_layout_edge_f1,
        },
        "passing": len(reports) - len(failures),
        "failing": len(failures),
        "minimum_soft_pixel_consistency": min(
            (item["soft_pixel_consistency"] for item in reports), default=0
        ),
        "minimum_normalized_similarity": min(
            (item["normalized_similarity"] for item in reports), default=0
        ),
        "minimum_layout_edge_f1": min(
            (item["layout_edge_f1"] for item in reports), default=0
        ),
        "minimum_layout_similarity": min(
            (item["layout_similarity"] for item in reports), default=0
        ),
        "minimum_focus_normalized_similarity": min(
            (
                item["focus_normalized_similarity"]
                for item in reports
                if "focus_normalized_similarity" in item
            ),
            default=0,
        ),
        "minimum_structural_f1": min(
            (item["structural_f1"] for item in reports), default=0
        ),
        "pages": reports,
    }
    (args.root / "report.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2) + "\n"
    )
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    if failures:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
