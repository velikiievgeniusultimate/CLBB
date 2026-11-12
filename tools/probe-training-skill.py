#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Coordinate one owned native watch and one existing bounded skill probe.

This emits gameplay input through probe-hero-control.py. The caller must select,
prepare and leave the expected training hero idle first. It never selects a hero,
changes settings or claims that successful process exits prove correct gameplay.
The native watcher runs with existing passwordless sudo when necessary.
"""
from __future__ import annotations

import argparse
from collections import Counter
import importlib.util
import json
import math
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
WATCH = ROOT / "mapper/runtime/target/debug/watch-skill-indicator"
PROBE = ROOT / "tools/probe-hero-control.py"
ANALYZER = ROOT / "tools/analyze-hero-probes.py"
spec = importlib.util.spec_from_file_location("existing_hero_probe", PROBE)
hero_probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hero_probe)


def selected_runtime():
    pid = json.loads((ROOT / "logs/current-atl-pid.json").read_text())["pid"]
    if type(pid) is not int or pid <= 0:
        raise RuntimeError("Invalid selected runtime PID")
    return pid, Path(f"/proc/{pid}").stat().st_uid


def wait_ready(process, hero, skill, pid, output, timeout=20.):
    """Use raw bounded reads: a partial line must not turn readline into a hang."""
    end = time.monotonic() + timeout
    pending = b""
    total = 0
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        while True:
            remaining = end - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("Native watcher did not announce READY in time")
            if not selector.select(remaining):
                raise TimeoutError("Native watcher did not announce READY in time")
            chunk = os.read(process.stdout.fileno(), 4096)
            if not chunk:
                raise RuntimeError("Native watcher exited before READY; inspect watch.stderr.txt")
            total += len(chunk)
            if total > 65536:
                raise RuntimeError("Native watcher READY protocol exceeded 64 KiB")
            output.write(chunk)
            output.flush()
            pending += chunk
            while b"\n" in pending:
                line, pending = pending.split(b"\n", 1)
                text = line.decode("utf-8", errors="strict")
                if not text.startswith("READY "):
                    continue
                fields = dict(part.split("=", 1) for part in text.split()[1:] if "=" in part)
                expected = {"hero": str(hero), "skill": str(skill), "pid": str(pid)}
                if any(fields.get(key) != value for key, value in expected.items()):
                    raise RuntimeError(f"Native READY identity does not match request: {text}")
                if process.poll() is not None:
                    raise RuntimeError("Native watcher exited immediately after READY")
                return text


def stop_watch(process):
    """Terminate only this Popen's new session, including its sudo monitor."""
    if process is None or process.poll() is not None:
        return
    try:
        if os.getpgid(process.pid) == process.pid:
            os.killpg(process.pid, signal.SIGTERM)
        else:
            process.terminate()
        process.wait(timeout=2)
    except ProcessLookupError:
        pass
    except subprocess.TimeoutExpired:
        if os.getpgid(process.pid) == process.pid:
            os.killpg(process.pid, signal.SIGKILL)
        else:
            process.kill()
        process.wait(timeout=2)


def finite_point(value):
    return (isinstance(value, list) and len(value) == 2
            and all(type(v) in (int, float) and math.isfinite(v) for v in value))


def late_delivery_metrics(probe):
    down, up = probe.get("skill_down_ns"), probe.get("physical_up_ns")
    rows = []
    for sample in probe.get("samples", []):
        end = sample.get("end_ns", 0)
        if (not down or not sample.get("stage", "").startswith("held:")
                or end < down + 300_000_000 or (up and end > up)):
            continue
        data = sample.get("data", {})
        inputs, aim = data.get("input", {}), data.get("aim", {})
        sequence = inputs.get("direction_matched_seq")
        current = (bool(inputs.get("direction_accepted")) and bool(inputs.get("direction_fresh"))
                   and aim.get("direction_sample_start_ns", 0) >= down
                   and type(sequence) is int and sequence > 0)
        rows.append({"stage": sample["stage"], "after_down_ms": (end - down) / 1e6,
                     "phase": "settled" if sample["stage"].endswith(":settled") else "immediate",
                     "matched_seq": sequence, "current_cast_receipt": current,
                     "sent_uv": inputs.get("sent"), "observed_direction": aim.get("direction_observed") if current else None})
    confirmed = [row for row in rows if row["current_cast_receipt"]]
    points = {tuple(row["sent_uv"]) for row in confirmed if finite_point(row["sent_uv"])}
    sequences = sorted({row["matched_seq"] for row in confirmed})
    return {"late_threshold_ms": 300, "sample_count": len(rows), "confirmed_current_cast_receipts": len(confirmed),
            "distinct_confirmed_sent_points": len(points), "matched_sequences": sequences, "rows": rows,
            "interpretation": "Causal directional receipts and sent points; this does not prove target hit accuracy. Short holds may have no late samples."}


def watch_integrity(path, pid, hero, skill):
    header, footer, errors = None, None, Counter()
    with path.open() as source:
        for line in source:
            row = json.loads(line)
            if row.get("event") == "header":
                if header is not None:
                    raise RuntimeError("Native watch has duplicate headers")
                header = row
            if row.get("event") == "summary":
                if footer is not None:
                    raise RuntimeError("Native watch has duplicate summaries")
                footer = row
            if row.get("event") == "source_error":
                errors["source: " + str(row.get("error"))] += 1
            if row.get("event") == "native" and isinstance(row.get("result"), dict) and "Err" in row["result"]:
                errors["native: " + str(row["result"]["Err"])] += 1
    checks = {"header_matches_request": isinstance(header, dict) and header.get("pid") == pid
              and header.get("hero") == hero and header.get("skill") == skill,
              "summary_present": isinstance(footer, dict),
              "same_match": isinstance(footer, dict) and footer.get("same_match") is True,
              "identity_unchanged": isinstance(footer, dict) and footer.get("identity_unchanged") is True}
    return checks, footer, dict(errors)


def run(args, report):
    directory = args.output_dir
    pid, owner = selected_runtime()
    report["pid"] = pid
    if not WATCH.is_file() or not ANALYZER.is_file():
        raise RuntimeError("Build watch-skill-indicator and provide analyze-hero-probes.py before probing")
    watch_file, probe_file = directory / "watch.jsonl", directory / "probe.json"
    duration = min(10000, max(5000, args.hold_ms + 4000))
    watch_command = ([] if os.geteuid() == 0 else ["sudo", "-n"]) + [str(WATCH), str(pid), str(args.skill), str(duration), str(watch_file)]
    command = [sys.executable, str(PROBE), "--hero", str(args.hero), "--expected-pid", str(pid), "--key", str(args.key),
               "--hold-ms", str(args.hold_ms), "--settle-ms", str(getattr(args, "settle_ms", 30)), "--output", str(probe_file)]
    if args.movement:
        command.append("--movement")
    if args.cancel:
        command.append("--cancel")
    report["commands"] = {"watch": watch_command, "probe": command}
    report["watch_duration_ms"] = duration
    watch = None
    try:
        with (directory / "watch.stdout.txt").open("xb") as stdout, (directory / "watch.stderr.txt").open("xb") as stderr:
            watch = subprocess.Popen(watch_command, cwd=ROOT, stdout=subprocess.PIPE, stderr=stderr, start_new_session=True)
            report["watch_child_pid"] = watch.pid
            report["ready"] = wait_ready(watch, args.hero, args.skill, pid, stdout)
            report["ready_ns"] = time.monotonic_ns()
            if selected_runtime()[0] != pid:
                raise RuntimeError("Selected runtime changed between watcher READY and probe start")
            # Existing probe owns all input and its own try/finally release path.
            # It already bounds every socket call and hold; do not SIGKILL its
            # cleanup just because the independent native watch finishes first.
            environment = os.environ.copy()
            environment["XDG_RUNTIME_DIR"] = f"/run/user/{owner}"
            with (directory / "probe.stdout.txt").open("xb") as probe_out, (directory / "probe.stderr.txt").open("xb") as probe_error:
                completed = subprocess.run(command, cwd=ROOT, env=environment, stdout=probe_out, stderr=probe_error, check=False)
            report["probe_returncode"] = completed.returncode
            remaining, _ = watch.communicate(timeout=duration / 1000. + 5)
            stdout.write(remaining)
            report["watch_returncode"] = watch.returncode
    finally:
        stop_watch(watch)
        if watch is not None and watch.stdout is not None:
            watch.stdout.close()
        report["watch_stopped"] = watch is None or watch.poll() is not None
    if not probe_file.is_file():
        raise RuntimeError("Probe did not produce its report; inspect probe.stderr.txt")
    probe = json.loads(probe_file.read_text())
    report["mechanical_checks"] = probe.get("mechanical_checks")
    report["probe_error"] = probe.get("error")
    report["cleanup_errors"] = probe.get("cleanup_errors")
    report["cleanup_contacts_empty"] = probe.get("cleanup_contacts_empty")
    report["probe_identity_matches_watch"] = probe.get("identity", {}).get("pid") == pid
    report["late_held_delivery"] = late_delivery_metrics(probe)
    if not watch_file.is_file():
        raise RuntimeError("Native watch did not produce its raw report")
    analysis = subprocess.run([sys.executable, str(ANALYZER), str(probe_file), str(watch_file)],
                              cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False, timeout=10)
    with (directory / "analysis.stderr.txt").open("xb") as output:
        output.write(analysis.stderr)
    if analysis.returncode:
        raise RuntimeError(f"Read-only analyzer failed ({analysis.returncode}); inspect analysis.stderr.txt")
    result = json.loads(analysis.stdout)
    with (directory / "analysis.json").open("x", encoding="utf-8") as output:
        json.dump(result, output, ensure_ascii=False, indent=2)
        output.write("\n")
    report["analysis"] = result
    checks, footer, errors = watch_integrity(watch_file, pid, args.hero, args.skill)
    report["watch_integrity_checks"], report["watch_summary"], report["watch_errors"] = checks, footer, errors
    trusted = all(checks.values()) and report["probe_identity_matches_watch"]
    report["actor_counter_evidence"] = [w for item in result for w in item.get("watch", [])] if trusted else []
    if not trusted:
        report["counter_evidence_limitation"] = "Watch completeness/identity is not confirmed; raw analyzer observations are retained but are not trusted counter evidence."


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hero", type=hero_probe.bounded_int(1, 10000), required=True)
    parser.add_argument("--key", type=hero_probe.key_code, required=True)
    parser.add_argument("--skill", type=hero_probe.bounded_int(1, 10_000_000), required=True)
    parser.add_argument("--hold-ms", type=hero_probe.bounded_int(16, 5000), default=600)
    parser.add_argument("--settle-ms", type=hero_probe.bounded_int(0, 250), default=30)
    parser.add_argument("--movement", action="store_true")
    parser.add_argument("--cancel", action="store_true")
    parser.add_argument("--output-dir", type=Path, required=True, help="Must be a new directory; existing evidence is never overwritten")
    args = parser.parse_args(argv)
    args.output_dir = args.output_dir.resolve()
    try:
        args.output_dir.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        parser.error(f"Cannot reserve a new report directory: {error}")
    report = {"format": "clbb-training-skill-probe", "version": 1, "status": "observation_only",
              "start_ns": time.monotonic_ns(), "request": {"hero": args.hero, "key": args.key, "skill": args.skill,
                  "hold_ms": args.hold_ms, "movement": args.movement, "cancel": args.cancel, "settle_ms": args.settle_ms},
              "limits": "Successful subprocess exits do not prove skill accuracy, application or cancellation. Inspect aligned local Actor receipts and explicit mechanical checks."}
    with (args.output_dir / "summary.json").open("x", encoding="utf-8") as output:
        try:
            run(args, report)
        except (Exception, KeyboardInterrupt) as error:
            report["error"] = f"{type(error).__name__}: {error}"
        finally:
            report["end_ns"] = time.monotonic_ns()
            json.dump(report, output, ensure_ascii=False, indent=2)
            output.write("\n")
    print(json.dumps({"summary": str(args.output_dir / "summary.json"), "error": report.get("error"),
                      "probe_error": report.get("probe_error"), "mechanical_checks": report.get("mechanical_checks"),
                      "actor_counter_evidence": report.get("actor_counter_evidence"),
                      "late_held_delivery": report.get("late_held_delivery")}, ensure_ascii=False))
    return int(bool(report.get("error") or report.get("probe_error") or report.get("cleanup_errors")
                    or report.get("cleanup_contacts_empty") is not True or report.get("probe_returncode") != 0
                    or report.get("watch_returncode") != 0
                    or report.get("probe_identity_matches_watch") is not True
                    or not all((report.get("watch_integrity_checks") or {"missing": False}).values())
                    or any(value is False for value in (report.get("mechanical_checks") or {}).values())))


if __name__ == "__main__":
    raise SystemExit(main())
