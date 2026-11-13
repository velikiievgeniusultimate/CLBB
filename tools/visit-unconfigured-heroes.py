#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Serial training coordinator with an explicit, fail-closed frozen-hero list.

The manifest must contain a nonempty ``heroes`` array of objects with integer
``id`` fields. Validation happens before importing the input-driving workflow.
This tool does not certify a hero: inspect the saved native/probe evidence.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path


def validate_request(heroes, manifest):
    rows = manifest.get("heroes") if isinstance(manifest, dict) else None
    if not isinstance(rows, list) or not rows:
        raise ValueError("A nonempty frozen-hero manifest is required")
    ids = [row.get("id") if isinstance(row, dict) else None for row in rows]
    if any(type(hero) is not int or hero <= 0 for hero in ids) or len(set(ids)) != len(ids):
        raise ValueError("Frozen hero IDs must be unique positive integers")
    if not heroes or any(type(hero) is not int or not 1 <= hero <= 9999 for hero in heroes):
        raise ValueError("Requested hero IDs must be positive integers in 1..9999")
    if len(set(heroes)) != len(heroes):
        raise ValueError("Duplicate requested heroes")
    overlap = sorted(set(ids).intersection(heroes))
    if overlap:
        raise ValueError(f"Refusing input for frozen heroes: {overlap}")
    return ids


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frozen-manifest", type=Path, required=True)
    parser.add_argument("--heroes", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--basic-probes", action="store_true")
    parser.add_argument("--cancel-probes", action="store_true")
    parser.add_argument("--combat-settle", action="store_true")
    parser.add_argument("--gm-close-point", nargs=2, type=float, default=(.974, .1))
    parser.add_argument("--dry-run", action="store_true", help="Validate only; never connect to the game")
    args = parser.parse_args()
    try:
        heroes = [int(part.strip()) for part in args.heroes.split(",")]
        manifest = json.loads(args.frozen_manifest.read_text())
        frozen = validate_request(heroes, manifest)
    except (ValueError, OSError) as error:
        parser.error(str(error))
    if args.dry_run:
        print(json.dumps({"heroes": heroes, "frozen_ids": frozen, "input_commands": 0}))
        return 0
    spec = importlib.util.spec_from_file_location("serial_hero_visits", Path(__file__).with_name("visit-training-heroes.py"))
    workflow = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(workflow)
    argv = ["--heroes", args.heroes, "--output-dir", str(args.output_dir),
            "--gm-close-point", *map(str, args.gm_close_point)]
    if args.basic_probes:
        argv.append("--basic-probes")
    if args.cancel_probes:
        argv.append("--cancel-probes")
    if args.combat_settle:
        argv.append("--combat-settle")
    return workflow.main(argv)


if __name__ == "__main__":
    raise SystemExit(main())
