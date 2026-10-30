#!/usr/bin/env python3
"""usage: compare.py REFERENCE_DIR RUNTIME_DIR"""
import collections
import csv
import json
import math
import statistics
import sys
from pathlib import Path

import numpy as np
from PIL import Image

CHANGED = 24 / 255
VISIBLE = 0.002
MATCH_IOU = 0.5


def rows(path):
    with open(path, newline="") as f:
        return [r for r in csv.reader(f, delimiter="\t")]


def tips(path):
    out = {}
    if not path.exists():
        return out
    for frame, tip, x, y, z in rows(path):
        out.setdefault(tip, {})[int(frame)] = np.array([float(x), float(y), float(z)])
    return out


def image(path):
    if not path.exists():
        return None
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.float32) / 255


def changed(a, b):
    return np.abs(a - b).max(axis=2) > CHANGED


def main(reference, runtime):
    reference, runtime = Path(reference), Path(runtime)
    if not (reference / "menu.tsv").exists():
        sys.exit(f"{reference / 'menu.tsv'} is missing: record the reference with the current harness")
    steps = {int(r[0]): {"start": int(r[1]), "frames": int(r[2]), "control": r[3], "label": r[4]} for r in rows(reference / "steps.tsv")}
    by_label = {s["label"]: s for s in steps.values()}
    window = range(by_label["motion"]["start"], by_label["settle"]["start"] + by_label["settle"]["frames"])

    lengths = {r[0]: float(r[1]) for r in rows(reference / "chains.tsv")[1:]}
    ref_tips, run_tips = tips(reference / "tips.tsv"), tips(runtime / "tips.tsv")
    per_tip = {}
    for tip, length in lengths.items():
        ref = ref_tips.get(tip, {})
        frames = [f for f in window if f in ref]
        if not frames or length <= 0:
            continue
        run = run_tips.get(tip, {})
        rest = ref[frames[0]]
        covered = sum(f in run for f in frames)
        err = [np.linalg.norm((run[f] if f in run else rest) - ref[f]) for f in frames]
        per_tip[tip] = {"matched": covered > 0, "frames": covered, "frames_expected": len(frames),
                        "rms_over_length": math.sqrt(sum(e * e for e in err) / len(err)) / length}

    renders = {int(r[2]): r[0] for r in rows(reference / "renders.tsv")}
    rest_name = renders[next(i for i, s in steps.items() if s["label"] == "settle")]
    ref_rest, run_rest = image(reference / rest_name), image(runtime / rest_name)
    table, controls, errors = [], {}, []
    for index, name in sorted(renders.items()):
        step = steps[index]
        ref, run = image(reference / name), image(runtime / name)
        error = None if run is None or ref.shape != run.shape else float(np.abs(run - ref).mean())
        if error is not None:
            errors.append(error)
        if step["control"] == "-":
            continue
        ref_mask = changed(ref, ref_rest)
        ref_area = float(ref_mask.mean())
        if run is None or run_rest is None or run.shape != ref.shape:
            run_area, iou, ok = None, None, False
        else:
            run_mask = changed(run, run_rest)
            run_area = float(run_mask.mean())
            if ref_area < VISIBLE and run_area < VISIBLE:
                iou, ok = None, True
            else:
                union = np.logical_or(ref_mask, run_mask).sum()
                iou = float(np.logical_and(ref_mask, run_mask).sum() / union) if union else 0.0
                ok = ref_area >= VISIBLE and run_area >= VISIBLE and iou >= MATCH_IOU
        controls.setdefault(step["control"], []).append(ok)
        table.append([step["control"], step["label"], name, f"{ref_area:.4f}", "-" if run_area is None else f"{run_area:.4f}",
                      "-" if iou is None else f"{iou:.3f}", "pass" if ok else "fail", "-" if error is None else f"{error:.4f}"])

    with open(runtime / "compare-steps.tsv", "w", newline="") as f:
        w = csv.writer(f, delimiter="\t", lineterminator="\n")
        w.writerow(["control", "step", "render", "reference changed", "runtime changed", "iou", "result", "image error"])
        w.writerows(table)

    offered = collections.Counter()
    if (runtime / "menu.tsv").exists():
        offered.update(r[0] for r in rows(runtime / "menu.tsv") if r)
    menu, listing = {}, []
    for path, named, kind, *_ in (r for r in rows(reference / "menu.tsv") if r):
        present = offered[named] > 0
        if present:
            offered[named] -= 1
        swept = controls.get(path)
        if not present:
            result = "fail: absent from runtime menu"
        elif swept is None:
            result = "pass: present" if kind == "SubMenu" else "pass: present, nothing to sweep"
        else:
            result = f"{'pass' if all(swept) else 'fail'}: {sum(swept)}/{len(swept)} steps match"
        menu[path] = result.startswith("pass")
        listing.append([named, kind, result])
    with open(runtime / "compare-controls.tsv", "w", newline="") as f:
        w = csv.writer(f, delimiter="\t", lineterminator="\n")
        w.writerow(["control", "kind", "result"])
        w.writerows(listing)
    builtin = {c: all(v) for c, v in controls.items() if c.startswith("builtin/")}
    errs = [t["rms_over_length"] for t in per_tip.values()]
    result = {
        "tips": len(per_tip),
        "tips_matched": sum(t["matched"] for t in per_tip.values()),
        "tip_rms_over_length_median": statistics.median(errs) if errs else None,
        "tip_rms_over_length_mean": statistics.fmean(errs) if errs else None,
        "menu_controls": len(menu),
        "menu_controls_matching": sum(menu.values()),
        "menu_match_fraction": sum(menu.values()) / len(menu) if menu else None,
        "builtin_inputs": len(builtin),
        "builtin_inputs_matching": sum(builtin.values()),
        "image_error_rest": None if ref_rest is None or run_rest is None or ref_rest.shape != run_rest.shape else float(np.abs(run_rest - ref_rest).mean()),
        "image_error_mean": statistics.fmean(errors) if errors else None,
        "renders_compared": len(errors),
        "renders_expected": len(renders),
        "per_tip": per_tip,
    }
    (runtime / "compare.json").write_text(json.dumps(result, indent=1, sort_keys=True) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "per_tip"}, sort_keys=True))


if __name__ == "__main__":
    main(*sys.argv[1:3])
