#!/usr/bin/env python3
"""Verify every Codex App Server P0 UI crop against a real CDP reference.

The manifest is intentionally flat: one entry is one independently gated
theme/surface/state.  A blocked entry documents missing real-app evidence and
keeps the matrix failing; it can never be mistaken for a passing synthetic
reference.
"""

from __future__ import annotations

import argparse
import json
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from PIL import Image, ImageChops, ImageEnhance


HARD_MIN_NORMALIZED_SIMILARITY = 99.5
THEMES = {"light", "dark"}
SURFACES = {
    "command_approval",
    "file_change_approval",
    "permissions_approval",
    "user_input",
    "file_change",
    "turn_diff",
    "permission_mode",
}
STATES = {
    "default",
    "hover_approve",
    "hover_decline",
    "focus",
    "loading",
    "approved",
    "declined",
    "timeout",
    "resolved",
    "error",
    "options",
    "options_focus",
    "started",
    "completed",
    "failed",
    "scrolled",
    "collapsed",
    "bottom",
    "question_1_default",
    "question_2_navigation",
    "previous_answer_persisted",
    "submitted_closed",
    "skipped",
    "dismissed",
}
STATUSES = {"ready", "blocked"}
PROJECT_ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class CropBox:
    x: int
    y: int
    width: int
    height: int

    @classmethod
    def parse(cls, value: Any, field: str) -> "CropBox":
        if (
            not isinstance(value, list)
            or len(value) != 4
            or any(not isinstance(part, int) for part in value)
        ):
            raise ValueError(f"{field} must be [x, y, width, height] integers")
        box = cls(*value)
        if box.x < 0 or box.y < 0 or box.width <= 0 or box.height <= 0:
            raise ValueError(f"{field} must have non-negative origin and positive size")
        return box

    def pillow(self) -> tuple[int, int, int, int]:
        return (self.x, self.y, self.x + self.width, self.y + self.height)

    def as_list(self) -> list[int]:
        return [self.x, self.y, self.width, self.height]


def fail(message: str) -> None:
    raise SystemExit(message)


def read_manifest(path: Path) -> dict[str, Any]:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"could not read P0 UI manifest {path}: {error}")
    if not isinstance(manifest, dict):
        fail("P0 UI manifest must be a JSON object")
    if manifest.get("version") != 1:
        fail("P0 UI manifest version must be 1")
    entries = manifest.get("entries")
    if not isinstance(entries, list) or not entries:
        fail("P0 UI manifest entries must be a non-empty JSON array")
    return manifest


def validate_source_audits(project_root: Path, manifest: dict[str, Any]) -> list[str]:
    """Require every audit named by the matrix to exist in this workspace."""

    errors: list[str] = []
    primary = manifest.get("source_audit")
    audits = manifest.get("source_audits")
    if not isinstance(audits, list) or not audits:
        return ["source_audits must be a non-empty array"]
    if primary not in audits:
        errors.append("source_audit must also appear in source_audits")
    seen: set[str] = set()
    for index, value in enumerate(audits):
        field = f"source_audits[{index}]"
        if not isinstance(value, str) or not value:
            errors.append(f"{field} must be a non-empty relative Markdown path")
            continue
        if value in seen:
            errors.append(f"{field} duplicates an earlier source audit: {value}")
        seen.add(value)
        relative = Path(value)
        if (
            relative.is_absolute()
            or ".." in relative.parts
            or relative.suffix.lower() != ".md"
        ):
            errors.append(f"{field} must be a safe relative .md path")
            continue
        if not (project_root / relative).is_file():
            errors.append(f"{field} does not exist: {value}")
    return errors


def relative_png(root: Path, value: Any, field: str, expected_kind: str) -> Path:
    if not isinstance(value, str) or not value:
        raise ValueError(f"{field} must be a non-empty relative PNG path")
    relative = Path(value)
    if relative.is_absolute() or ".." in relative.parts or relative.suffix.lower() != ".png":
        raise ValueError(f"{field} must be a safe relative .png path")
    if not relative.parts or relative.parts[0] != expected_kind:
        raise ValueError(f"{field} must be stored below {expected_kind}/")
    return root / relative


def validate_manifest(
    root: Path, manifest: dict[str, Any]
) -> tuple[list[dict[str, Any]], list[str], set[Path], set[Path]]:
    parsed: list[dict[str, Any]] = []
    errors: list[str] = []
    identities: set[tuple[str, str, str, str]] = set()
    ids: set[str] = set()
    expected_reference: set[Path] = set()
    expected_actual: set[Path] = set()

    required = manifest.get("required_scenarios")
    expected_identities: set[tuple[str, str, str, str]] = set()
    if not isinstance(required, list) or not required:
        errors.append("required_scenarios must be a non-empty array")
    else:
        scenario_keys: set[tuple[str, str]] = set()
        for index, raw_scenario in enumerate(required):
            prefix = f"required_scenarios[{index}]"
            if not isinstance(raw_scenario, dict):
                errors.append(f"{prefix} must be an object")
                continue
            surface = raw_scenario.get("surface")
            variant = raw_scenario.get("variant", "default")
            states = raw_scenario.get("states")
            if surface not in SURFACES:
                errors.append(f"{prefix}.surface must be one of {sorted(SURFACES)}")
                continue
            if not isinstance(variant, str) or not variant:
                errors.append(f"{prefix}.variant must be a non-empty string")
                continue
            key = (surface, variant)
            if key in scenario_keys:
                errors.append(f"duplicate required scenario: {key}")
            scenario_keys.add(key)
            if (
                not isinstance(states, list)
                or not states
                or any(state not in STATES for state in states)
            ):
                errors.append(
                    f"{prefix}.states must be a non-empty array drawn from {sorted(STATES)}"
                )
                continue
            if len(set(states)) != len(states):
                errors.append(f"{prefix}.states must not contain duplicates")
            for theme in THEMES:
                for state in states:
                    expected_identities.add((theme, surface, variant, state))

    for index, raw in enumerate(manifest["entries"]):
        prefix = f"entries[{index}]"
        if not isinstance(raw, dict):
            errors.append(f"{prefix} must be an object")
            continue
        entry_id = raw.get("id")
        theme = raw.get("theme")
        surface = raw.get("surface")
        state = raw.get("state")
        variant = raw.get("variant", "default")
        status = raw.get("status")
        if not isinstance(entry_id, str) or not entry_id:
            errors.append(f"{prefix}.id must be a non-empty string")
            continue
        if entry_id in ids:
            errors.append(f"duplicate entry id: {entry_id}")
        ids.add(entry_id)
        if theme not in THEMES:
            errors.append(f"{entry_id}: theme must be one of {sorted(THEMES)}")
        if surface not in SURFACES:
            errors.append(f"{entry_id}: surface must be one of {sorted(SURFACES)}")
        if state not in STATES:
            errors.append(f"{entry_id}: state must be one of {sorted(STATES)}")
        if not isinstance(variant, str) or not variant:
            errors.append(f"{entry_id}: variant must be a non-empty string")
        if status not in STATUSES:
            errors.append(f"{entry_id}: status must be one of {sorted(STATUSES)}")
        identity = (str(theme), str(surface), str(variant), str(state))
        if identity in identities:
            errors.append(
                f"{entry_id}: duplicate theme/surface/variant/state identity {identity}"
            )
        identities.add(identity)

        if status == "blocked":
            blocker = raw.get("blocker")
            if not isinstance(blocker, str) or not blocker.strip():
                errors.append(f"{entry_id}: blocked entry requires a non-empty blocker")
            forbidden = {
                "reference",
                "actual",
                "reference_box",
                "actual_box",
            }.intersection(raw)
            if forbidden:
                errors.append(
                    f"{entry_id}: blocked entry must not provide synthetic evidence fields: "
                    + ", ".join(sorted(forbidden))
                )
            parsed.append(dict(raw))
            continue

        try:
            reference = relative_png(root, raw.get("reference"), "reference", "reference")
            actual = relative_png(root, raw.get("actual"), "actual", "actual")
            reference_box = CropBox.parse(raw.get("reference_box"), "reference_box")
            actual_box = CropBox.parse(raw.get("actual_box"), "actual_box")
            if (reference_box.width, reference_box.height) != (
                actual_box.width,
                actual_box.height,
            ):
                raise ValueError("reference_box and actual_box must have equal crop size")
        except ValueError as error:
            errors.append(f"{entry_id}: {error}")
            continue
        expected_reference.add(reference)
        expected_actual.add(actual)
        parsed.append(
            {
                **raw,
                "reference_path": reference,
                "actual_path": actual,
                "reference_crop": reference_box,
                "actual_crop": actual_box,
            }
        )

    missing_identities = sorted(expected_identities - identities)
    unexpected_identities = sorted(identities - expected_identities)
    if missing_identities:
        errors.append(
            "matrix is missing required theme/surface/variant/state entries: "
            + ", ".join(map(str, missing_identities))
        )
    if unexpected_identities:
        errors.append(
            "matrix contains entries outside required_scenarios: "
            + ", ".join(map(str, unexpected_identities))
        )
    return parsed, errors, expected_reference, expected_actual


def all_pngs(directory: Path) -> set[Path]:
    if not directory.is_dir():
        return set()
    return {path for path in directory.rglob("*.png") if path.is_file()}


def validate_file_sets(
    root: Path, expected_reference: set[Path], expected_actual: set[Path]
) -> list[str]:
    errors: list[str] = []
    for kind, expected in (
        ("reference", expected_reference),
        ("actual", expected_actual),
    ):
        directory = root / kind
        found = all_pngs(directory)
        non_png_entries = (
            sorted(
                path
                for path in directory.rglob("*")
                if path.is_file() and path.suffix.lower() != ".png"
            )
            if directory.is_dir()
            else []
        )
        missing = sorted(expected - found)
        unexpected = sorted(found - expected)
        if missing:
            errors.append(
                f"{kind} missing: "
                + ", ".join(str(path.relative_to(root)) for path in missing)
            )
        if unexpected:
            errors.append(
                f"{kind} unexpected: "
                + ", ".join(str(path.relative_to(root)) for path in unexpected)
            )
        if non_png_entries:
            errors.append(
                f"{kind} contains non-PNG evidence files: "
                + ", ".join(str(path.relative_to(root)) for path in non_png_entries)
            )
    return errors


def checked_crop(path: Path, box: CropBox) -> Image.Image:
    try:
        with Image.open(path) as image:
            if image.format != "PNG":
                raise ValueError(f"{path} is {image.format}, expected PNG")
            width, height = image.size
            if box.x + box.width > width or box.y + box.height > height:
                raise ValueError(
                    f"{path}: crop {box.as_list()} exceeds image {width}x{height}"
                )
            return image.convert("RGB").crop(box.pillow())
    except OSError as error:
        raise ValueError(f"could not read {path}: {error}") from error


def compare_entry(entry: dict[str, Any], output_root: Path, minimum: float) -> dict[str, Any]:
    reference = checked_crop(entry["reference_path"], entry["reference_crop"])
    actual = checked_crop(entry["actual_path"], entry["actual_crop"])
    if reference.size != actual.size:
        raise ValueError(
            f"{entry['id']}: crop sizes differ: reference={reference.size}, actual={actual.size}"
        )
    difference = ImageChops.difference(reference, actual)
    pixels = list(difference.get_flattened_data())
    absolute_error = sum(sum(pixel) for pixel in pixels)
    normalized_similarity = 100.0 * (
        1.0 - absolute_error / (len(pixels) * 3 * 255)
    )
    output = output_root / entry["theme"] / entry["id"]
    output.mkdir(parents=True, exist_ok=True)
    reference.save(output / "reference-crop.png")
    actual.save(output / "actual-crop.png")
    difference.save(output / "diff.png")
    ImageEnhance.Contrast(difference).enhance(4.0).save(output / "diff-4x.png")
    Image.blend(reference, actual, 0.5).save(output / "overlay.png")
    report = {
        "id": entry["id"],
        "theme": entry["theme"],
        "surface": entry["surface"],
        "variant": entry.get("variant", "default"),
        "state": entry["state"],
        "status": "ready",
        "reference": str(entry["reference_path"]),
        "actual": str(entry["actual_path"]),
        "reference_box": entry["reference_crop"].as_list(),
        "actual_box": entry["actual_crop"].as_list(),
        "crop_size": list(reference.size),
        "normalized_similarity": normalized_similarity,
        "minimum_normalized_similarity": minimum,
        "passing": normalized_similarity >= minimum,
    }
    (output / "report.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return report


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="gpui-p0-ui-gate-") as temporary:
        root = Path(temporary)
        (root / "reference" / "dark").mkdir(parents=True)
        (root / "actual" / "dark").mkdir(parents=True)
        reference = Image.new("RGB", (8, 8), (20, 20, 20))
        actual = reference.copy()
        reference.save(root / "reference" / "dark" / "same.png")
        actual.save(root / "actual" / "dark" / "same.png")
        manifest = {
            "version": 1,
            "required_scenarios": [
                {
                    "surface": "command_approval",
                    "variant": "network",
                    "states": ["default"],
                }
            ],
            "entries": [
                {
                    "id": "self-test",
                    "theme": "dark",
                    "surface": "command_approval",
                    "variant": "network",
                    "state": "default",
                    "status": "ready",
                    "reference": "reference/dark/same.png",
                    "actual": "actual/dark/same.png",
                    "reference_box": [0, 0, 8, 8],
                    "actual_box": [0, 0, 8, 8],
                },
                {
                    "id": "self-test-light",
                    "theme": "light",
                    "surface": "command_approval",
                    "variant": "network",
                    "state": "default",
                    "status": "blocked",
                    "blocker": "self-test deliberately exercises blocked evidence",
                }
            ],
        }
        parsed, errors, expected_reference, expected_actual = validate_manifest(
            root, manifest
        )
        assert not errors, errors
        assert not validate_file_sets(root, expected_reference, expected_actual)
        report = compare_entry(parsed[0], root / "diff", 99.5)
        assert report["normalized_similarity"] == 100.0
        assert report["passing"] is True

        blocked_manifest = {
            "version": 1,
            "required_scenarios": [
                {
                    "surface": "user_input",
                    "states": ["timeout"],
                }
            ],
            "entries": [
                {
                    "id": "blocked",
                    "theme": "light",
                    "surface": "user_input",
                    "state": "timeout",
                    "status": "blocked",
                    "blocker": "real app state could not be triggered",
                },
                {
                    "id": "blocked-dark",
                    "theme": "dark",
                    "surface": "user_input",
                    "state": "timeout",
                    "status": "blocked",
                    "blocker": "real app state could not be triggered",
                }
            ],
        }
        parsed, errors, _, _ = validate_manifest(root, blocked_manifest)
        assert not errors, errors
        assert parsed[0]["status"] == "blocked"

        (root / "AUDIT.md").write_text("# Self-test audit\n", encoding="utf-8")
        audit_manifest = {
            "source_audit": "AUDIT.md",
            "source_audits": ["AUDIT.md"],
        }
        assert not validate_source_audits(root, audit_manifest)
        audit_manifest["source_audits"].append("missing/AUDIT.md")
        assert validate_source_audits(root, audit_manifest) == [
            "source_audits[1] does not exist: missing/AUDIT.md"
        ]
    print("P0 UI matrix verifier self-test passed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("artifacts/p0-ui-matrix"))
    parser.add_argument(
        "--manifest",
        type=Path,
        default=PROJECT_ROOT / "scripts" / "p0_ui_matrix_manifest.json",
    )
    parser.add_argument(
        "--min-normalized-similarity",
        type=float,
        default=HARD_MIN_NORMALIZED_SIMILARITY,
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if args.min_normalized_similarity < HARD_MIN_NORMALIZED_SIMILARITY:
        parser.error(
            "--min-normalized-similarity cannot be lower than the 99.5% hard gate"
        )
    if args.min_normalized_similarity > 100.0:
        parser.error("--min-normalized-similarity cannot exceed 100")

    root = args.root.resolve()
    manifest = read_manifest(args.manifest)
    parsed, errors, expected_reference, expected_actual = validate_manifest(root, manifest)
    errors.extend(validate_source_audits(PROJECT_ROOT, manifest))
    errors.extend(validate_file_sets(root, expected_reference, expected_actual))
    if errors:
        fail("invalid P0 UI matrix:\n- " + "\n- ".join(errors))

    reports: list[dict[str, Any]] = []
    for entry in parsed:
        if entry["status"] == "blocked":
            reports.append(
                {
                    "id": entry["id"],
                    "theme": entry["theme"],
                    "surface": entry["surface"],
                    "variant": entry.get("variant", "default"),
                    "state": entry["state"],
                    "status": "blocked",
                    "blocker": entry["blocker"],
                    "passing": False,
                }
            )
            continue
        try:
            reports.append(
                compare_entry(
                    entry,
                    root / "diff",
                    args.min_normalized_similarity,
                )
            )
        except ValueError as error:
            fail(f"invalid P0 UI evidence for {entry['id']}: {error}")

    failures = [report for report in reports if not report["passing"]]
    ready = [report for report in reports if report["status"] == "ready"]
    blocked = [report for report in reports if report["status"] == "blocked"]
    summary = {
        "acceptance": {
            "definition": (
                "normalized_similarity = 100 * (1 - sum(abs(reference RGB - actual RGB)) "
                "/ (crop width * crop height * 3 * 255)); every individual real-CDP/GPUI "
                "theme/surface/state crop must pass, and blocked evidence keeps the gate closed"
            ),
            "minimum_normalized_similarity": args.min_normalized_similarity,
        },
        "entries": len(reports),
        "ready": len(ready),
        "blocked": len(blocked),
        "passing": len(reports) - len(failures),
        "failing": len(failures),
        "minimum_observed_normalized_similarity": min(
            (report["normalized_similarity"] for report in ready), default=None
        ),
        "all_entries_passing": not failures,
        "results": reports,
    }
    root.mkdir(parents=True, exist_ok=True)
    (root / "report.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    if failures:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
