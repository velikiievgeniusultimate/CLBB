#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Bounded native-touch baseline for one current training button.

This emits game input. It never chooses/prepares a hero, changes settings, moves
the desktop cursor, or uses mapper begin/end. The native hold command owns its
one fixed UP timer; move-held cannot extend it. Run only with exclusive input.
The separate native watcher must announce the exact requested identity first.
Exit0 means a complete observation with confirmed cleanup, not correct gameplay.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("native_touch_watch", ROOT / "tools/probe-training-skill.py")
watch_tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(watch_tool)
hero_probe = watch_tool.hero_probe
WATCH = watch_tool.WATCH
RAW_GRACE_SECONDS = 2.


def finite_uv(value):
    return (isinstance(value, (list, tuple)) and len(value) == 2
            and all(type(v) in (int, float) and math.isfinite(v) and 0 <= v <= 1 for v in value))


def parse_moves(value):
    try:
        moves = json.loads(value)
    except (ValueError, TypeError) as error:
        raise argparse.ArgumentTypeError("Use a JSON array of1..16 UV pairs") from error
    if not isinstance(moves, list) or not 1 <= len(moves) <= 16 or not all(finite_uv(point) for point in moves):
        raise argparse.ArgumentTypeError("Use a JSON array of1..16 finite UV pairs in0..1")
    return moves


def make_schedule(hold_ms, moves, cancel_uv=None):
    if type(hold_ms) is not int or not 16 <= hold_ms <= 5000:
        raise ValueError("Hold must be16..5000ms")
    if not isinstance(moves, list) or not 1 <= len(moves) <= 16 or not all(finite_uv(point) for point in moves):
        raise ValueError("Expected1..16 finite UV waypoints")
    if cancel_uv is not None and not finite_uv(cancel_uv):
        raise ValueError("Explicit cancel UV must be finite and inside0..1")
    rows = [("move", point) for point in moves]
    if cancel_uv is not None:
        rows.append(("cancel", cancel_uv))
    first = min(80., hold_ms / 4.)
    last = hold_ms - first
    return [{"offset_ms": first + (last - first) * index / max(1, len(rows) - 1),
             "uv": list(point), "kind": kind} for index, (kind, point) in enumerate(rows)]


def number(value):
    if not isinstance(value, dict) or type(value.get("kind")) is not int or value["kind"] != 1:
        raise RuntimeError("Native numeric field is missing/untyped")
    result = value.get("value")
    if type(result) not in (int, float) or not math.isfinite(result):
        raise RuntimeError("Native numeric field is invalid")
    return result


def text(value):
    if (not isinstance(value, dict) or type(value.get("present")) is not int or value["present"] != 1
            or type(value.get("size")) is not int or not 0 <= value["size"] <= 256
            or not isinstance(value.get("value"), str) or len(value["value"].encode()) != value["size"]):
        raise RuntimeError("Native text field is missing/untyped")
    return value["value"]


def true_flag(value):
    return isinstance(value, dict) and type(value.get("value")) is int and value["value"] == 1


def active_slot(ui):
    value = ui.get("active_slot")
    # Source leaves exactly the default Text when no button is active; its
    # validated ready UI is the authority for this particular empty sentinel.
    if (isinstance(value, dict) and type(value.get("present")) is int and type(value.get("size")) is int
            and value == {"present": 0, "size": 0, "value": ""}):
        return ""
    return text(value)


def vector2(value):
    if not isinstance(value, dict) or type(value.get("count")) is not int or value["count"] != 2:
        raise RuntimeError("Native UV vector is missing/untyped")
    rows = value.get("values")
    if not isinstance(rows, list) or len(rows) != 2:
        raise RuntimeError("Native UV vector has invalid dimension")
    point = [number(row) for row in rows]
    if not finite_uv(point):
        raise RuntimeError("Native UV vector is outside the game surface")
    return point


def validate_native_button(data, hero, slot, skill, now_ns=None, epoch=None):
    if (not isinstance(data, dict) or data.get("fresh") is not True
            or type(data.get("hero")) is not int or data["hero"] != hero):
        raise RuntimeError("Fresh native frame for the expected hero is unavailable")
    frame = data.get("frame", {})
    ui = frame.get("ui", {})
    if (not true_flag(frame.get("ready")) or not true_flag(ui.get("ready"))
            or number(frame.get("hero_id")) != hero or number(ui.get("hero_id")) != hero
            or not true_flag(frame.get("token_nonempty")) or not true_flag(frame.get("token_string"))):
        raise RuntimeError("Native frame/UI ownership is not ready")
    token = text(frame.get("token"))
    if not token or data.get("epoch") != token or (epoch is not None and token != epoch):
        raise RuntimeError("Native match epoch changed or is unknown")
    end = number(frame.get("time_ns")); duration = number(frame.get("sample_duration_ms"))
    now = time.monotonic_ns() if now_ns is None else now_ns
    if (end < 0 or not 0 <= duration <= 100 or number(ui.get("time_ns")) != end
            or end > now + 10_000_000 or now - end > 100_000_000):
        raise RuntimeError("Native frame is stale or its read bracket is invalid")
    rows = ui.get("buttons")
    if (not isinstance(rows, list) or not 1 <= len(rows) <= 32
            or type(ui.get("button_count")) is not int or ui["button_count"] != len(rows)):
        raise RuntimeError("Native button inventory is invalid")
    matches = [row for row in rows if text(row.get("slot")) == slot]
    if len(matches) != 1:
        raise RuntimeError("Requested native button is missing or ambiguous")
    button = matches[0]
    base = number(button.get("skill_id"))
    if int(base) != base:
        raise RuntimeError("Native skill identity is not integral")
    if not 1 <= base <= 10_000_000 or not true_flag(button.get("active")):
        raise RuntimeError("Requested native button is inactive or has no skill identity")
    state = frame.get("skill_state", {})
    variants = state.get("buttons", [])
    if not isinstance(variants, list) or len(variants) > 32:
        raise RuntimeError("Native skill state inventory is invalid")
    variants = [row for row in variants if text(row.get("slot")) == slot]
    if len(variants) > 1:
        raise RuntimeError("Native current form is ambiguous")
    active = int(base)
    if variants:
        if (not true_flag(state.get("ready")) or number(state.get("hero_id")) != hero
                or text(state.get("token")) != token or number(state.get("sample_end_ns")) != end):
            raise RuntimeError("Native current form belongs to another or unready observation")
        if number(variants[0].get("base_skill_id")) != base:
            raise RuntimeError("Native button/form base identity differs")
        form = number(variants[0].get("active_skill_id"))
        if not 1 <= form <= 10_000_000 or int(form) != form:
            raise RuntimeError("Native active skill identity is invalid")
        active = int(form)
    if skill not in (int(base), active):
        raise RuntimeError(f"Expected skill{skill} is neither this native button's base nor current form")
    return {"slot": slot, "base_skill": int(base), "active_skill": active,
            "press": vector2(button.get("press_uv")), "epoch": token,
            "geometry_source": text(button.get("geometry_source")), "sample_end_ns": end}


class NativeTouchProbe(hero_probe.Probe):
    def __init__(self, hero, report, expected_pid):
        super().__init__(hero, report, expected_pid)
        owner = Path(f"/proc/{self.pid}").stat().st_uid
        self.socket_path = Path(f"/run/user/{owner}/clbb/mapper-{self.pid}.sock")

    def command(self, command, deadline=None, cleanup=False):
        if command in ("begin", "end") or command.startswith(("pointer ", "key ", "mouse ")):
            raise RuntimeError("Native baseline must not reset or invoke mapper input")
        if command.startswith(("hold ", "move-held ")):
            deadline = min(time.monotonic() + 1., deadline if deadline is not None else float("inf"))
            self.identity()
            current = self.read("hero.id", deadline=deadline)
            if type(current) is not int or current != self.hero:
                raise RuntimeError(f"Expected hero{self.hero}, current hero is{current}; refusing native input")
        return super().command(command, deadline=deadline, cleanup=cleanup)


def sample(probe, report, stage, deadline=None):
    result = {"stage": stage, "start_ns": time.monotonic_ns(), "data": {}}
    report.setdefault("samples", []).append(result)
    try:
        for name in ("delivery", "contacts", "native_frame", "input", "cancel"):
            result["data"][name] = probe.read("diagnostics." + name, deadline=deadline)
        return result
    finally:
        result["end_ns"] = time.monotonic_ns()


def expected_hero(probe, hero, deadline=None):
    probe.identity()
    value = probe.read("hero.id", deadline=deadline)
    if type(value) is not int or value != hero:
        raise RuntimeError(f"Expected hero{hero}, observed{value}; refusing native input")


def idle(probe, hero):
    expected_hero(probe, hero)
    if probe.read("diagnostics.contacts").get("active") != []:
        raise RuntimeError("Mapped contacts are held or unknown; refusing native input")
    if probe.read("diagnostics.delivery").get("raw_gesture_active") is not False:
        raise RuntimeError("Raw gesture is active or unknown; refusing native input")
    inputs = probe.read("diagnostics.input")
    if (inputs.get("hero") != hero or type(inputs.get("touch")) is not int or inputs["touch"] >= 0
            or inputs.get("pending_release") not in (0, False)):
        raise RuntimeError("Mapped skill contact/release is active or unknown")
    if probe.read("diagnostics.gestures").get("active") != []:
        raise RuntimeError("Linked contacts are held or unknown; refusing native input")


def wait_until(deadline):
    while time.monotonic() < deadline:
        time.sleep(max(0., min(.025, deadline - time.monotonic())))


def wait_natural_up(probe, report, deadline, not_before=None):
    while True:
        probe.identity()
        state = probe.read("diagnostics.delivery", deadline=deadline)
        row = {"ns": time.monotonic_ns(), "delivery": state}
        report.setdefault("natural_up_polls", []).append(row)
        if state.get("raw_gesture_active") is False and (not_before is None or time.monotonic() >= not_before):
            report.setdefault("natural_up_observed_ns", row["ns"])
            report["cleanup_raw_idle"] = True
            report["cleanup_last_touch_status"] = state.get("last_touch_status")
            report["native_up_dispatch_acknowledged"] = state.get("last_touch_status") == "dispatched"
            return
        if type(state.get("raw_gesture_active")) is not bool:
            raise RuntimeError("Raw delivery state is unknown; natural UP was not confirmed")
        if time.monotonic() >= deadline:
            raise TimeoutError("Native hold timer did not finish within the cleanup bound")
        wait_until(min(deadline, time.monotonic() + .025))


def run_input(args, report, probe):
    idle(probe, args.hero)
    initial = sample(probe, report, "preflight")
    native = initial["data"]["native_frame"]
    button = validate_native_button(native, args.hero, args.slot, args.skill)
    if active_slot(native["frame"]["ui"]):
        raise RuntimeError("A native skill is already active; refusing to interrupt it")
    moves = args.moves
    if moves is None:
        u, v = button["press"]
        moves = [[min(.98, u + .12), v], [u, min(.98, v + .12)],
                 [max(.02, u - .12), v], [u, max(.02, v - .12)]]
    schedule = make_schedule(args.hold_ms, moves, args.cancel_uv)
    report["native_button"], report["schedule"] = button, schedule
    attempted = False
    cleanup_deadline = None
    try:
        # Revalidate immediately before DOWN, after the native watcher is READY.
        idle(probe, args.hero)
        current = probe.read("diagnostics.native_frame")
        current_button = validate_native_button(current, args.hero, args.slot, args.skill, epoch=button["epoch"])
        if active_slot(current["frame"]["ui"]):
            raise RuntimeError("A native skill became active before DOWN")
        if current_button["base_skill"] != button["base_skill"] or current_button["press"] != button["press"]:
            raise RuntimeError("Native button identity/press changed before DOWN")
        expected_hero(probe, args.hero)
        down_at = time.monotonic()
        report["hold_attempt_ns"] = time.monotonic_ns()
        hold_deadline = down_at + args.hold_ms / 1000.
        cleanup_deadline = hold_deadline + 1. + RAW_GRACE_SECONDS
        report["hold_deadline_ns"] = round(hold_deadline * 1e9)
        attempted = True  # A lost hold reply still requires waiting for natural UP.
        reply = probe.command(f"hold {button['press'][0]:.12f} {button['press'][1]:.12f} {args.hold_ms}")
        if reply != "OK hold":
            raise RuntimeError(f"Native hold was not accepted: {reply}")
        report["hold_ack_ns"] = time.monotonic_ns()
        report["hold_accepted"] = True
        for index, point in enumerate(schedule):
            target = down_at + point["offset_ms"] / 1000.
            wait_until(target)
            if time.monotonic() >= hold_deadline:
                report.setdefault("skipped_moves", []).append({**point, "reason": "original_hold_deadline_elapsed"})
                continue
            expected_hero(probe, args.hero, deadline=hold_deadline)
            if probe.read("diagnostics.delivery", deadline=hold_deadline).get("raw_gesture_active") is not True:
                raise RuntimeError("Native finger ended before the requested held MOVE")
            start = time.monotonic_ns()
            reply = probe.command(f"move-held {point['uv'][0]:.12f} {point['uv'][1]:.12f}", deadline=hold_deadline)
            report.setdefault("move_receipts", []).append({**point, "index": index,
                "command_start_ns": start, "command_end_ns": time.monotonic_ns(), "reply": reply})
            if reply != "OK move-held":
                raise RuntimeError(f"Native held MOVE was not accepted: {reply}")
            # Sampling is optional inside the fixed native timer: no diagnostic
            # timeout may restart the finger or postpone its automatic UP.
            try:
                sample(probe, report, f"held:{index}:{point['kind']}", deadline=hold_deadline)
            except TimeoutError:
                if time.monotonic() < hold_deadline:
                    raise
                report["samples"][-1]["truncated_at_hold_deadline"] = True
                break
        wait_natural_up(probe, report, cleanup_deadline)
        report["mechanical_checks"] = {
            "hold_acknowledged": True,
            "all_requested_moves_acknowledged": len(report.get("move_receipts", [])) == len(schedule),
            "natural_up_observed": True,
            "native_up_dispatch_acknowledged": report.get("native_up_dispatch_acknowledged") is True,
            "cancel_drag_requested": args.cancel_uv is not None,
            "cancellation_result": "not_inferred_from_drag_or_exit_status" if args.cancel_uv is not None else None}
    except (Exception, KeyboardInterrupt) as error:
        report["input_error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        errors = []
        if attempted:
            try:
                # A lost DOWN reply is not proof the command was rejected. Wait
                # at least the original requested timer before accepting idle.
                wait_natural_up(probe, report, cleanup_deadline,
                                None if report.get("hold_accepted") else hold_deadline)
                expected_hero(probe, args.hero, deadline=cleanup_deadline)
                report["cleanup_contacts_empty"] = probe.read("diagnostics.contacts", deadline=cleanup_deadline).get("active") == []
                if report["cleanup_contacts_empty"] is not True:
                    raise RuntimeError("Mapped contacts were not empty after native UP")
                final = sample(probe, report, "after_natural_up", deadline=cleanup_deadline)
                report["final_native_frame_available"] = final["data"]["native_frame"].get("fresh") is True
            except (Exception, KeyboardInterrupt) as error:
                errors.append(f"{type(error).__name__}: {error}")
        report["cleanup_errors"] = errors
        if errors:
            raise RuntimeError("Native probe cleanup is unconfirmed: " + "; ".join(errors))


def run(args, report):
    pid, _ = watch_tool.selected_runtime()
    if args.expected_pid is not None and args.expected_pid != pid:
        raise RuntimeError("Selected runtime differs from --expected-pid")
    probe = NativeTouchProbe(args.hero, report, pid)
    idle(probe, args.hero)
    if not WATCH.is_file():
        raise RuntimeError("Build watch-skill-indicator before probing")
    duration = min(10000, max(5000, args.hold_ms + 4000))
    watch_path = args.output_dir / "watch.jsonl"
    command = ([] if os.geteuid() == 0 else ["sudo", "-n"]) + [str(WATCH), str(pid), str(args.skill), str(duration), str(watch_path)]
    report["watch_command"] = command
    process = None
    try:
        with (args.output_dir / "watch.stdout.txt").open("xb") as stdout, (args.output_dir / "watch.stderr.txt").open("xb") as stderr:
            process = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=stderr, start_new_session=True)
            report["watch_child_pid"] = process.pid
            report["ready"] = watch_tool.wait_ready(process, args.hero, args.skill, pid, stdout)
            report["ready_ns"] = time.monotonic_ns()
            probe.identity()
            run_input(args, report, probe)
            remaining, _ = process.communicate(timeout=duration / 1000. + 3.)
            stdout.write(remaining)
            report["watch_returncode"] = process.returncode
    finally:
        watch_tool.stop_watch(process)
        if process is not None and process.stdout is not None:
            process.stdout.close()
        report["watch_stopped"] = process is None or process.poll() is not None
    checks, footer, errors = watch_tool.watch_integrity(watch_path, pid, args.hero, args.skill)
    report["watch_integrity_checks"], report["watch_summary"], report["watch_errors"] = checks, footer, errors


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hero", type=hero_probe.bounded_int(1, 9999), required=True)
    parser.add_argument("--slot", required=True, help="Exact current native slot, e.g.skill:0 or attack:0")
    parser.add_argument("--skill", type=hero_probe.bounded_int(1, 10_000_000), required=True)
    parser.add_argument("--expected-pid", type=hero_probe.bounded_int(1, 0x7fffffff))
    parser.add_argument("--hold-ms", type=hero_probe.bounded_int(16, 5000), default=700)
    parser.add_argument("--moves", type=parse_moves, help="JSON UV waypoints; default4 offsets around the current native press")
    parser.add_argument("--cancel-uv", type=float, nargs=2, help="Explicit final cancel drag point; never inferred automatically")
    parser.add_argument("--output-dir", type=Path, required=True, help="Must be a new directory")
    args = parser.parse_args(argv)
    if not args.slot or len(args.slot) > 64 or any(character.isspace() for character in args.slot):
        parser.error("Use one exact native slot of1..64 characters")
    if args.cancel_uv is not None and not finite_uv(args.cancel_uv):
        parser.error("--cancel-uv requires finite UV values in0..1")
    args.output_dir = args.output_dir.resolve()
    try:
        args.output_dir.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        parser.error(f"Cannot reserve new output directory: {error}")
    report = {"format": "clbb-native-touch-probe", "version": 1, "status": "observation_only",
        "start_ns": time.monotonic_ns(), "commands": [], "samples": [],
        "request": {"hero": args.hero, "slot": args.slot, "skill": args.skill, "hold_ms": args.hold_ms,
                    "moves": args.moves, "cancel_uv": args.cancel_uv},
        "limits": "Raw baseline only. No mapper begin/end, movement, settings or hero changes. Native actor/registry receipts have independent read brackets. Exit0 does not prove application, accuracy or cancellation."}
    with (args.output_dir / "summary.json").open("x", encoding="utf-8") as output:
        try:
            run(args, report)
        except (Exception, KeyboardInterrupt) as error:
            report["error"] = f"{type(error).__name__}: {error}"
        finally:
            report["end_ns"] = time.monotonic_ns()
            json.dump(report, output, ensure_ascii=False, indent=2)
            output.write("\n")
    code = int(bool(report.get("error") or report.get("cleanup_errors")
        or report.get("cleanup_contacts_empty") is not True or report.get("cleanup_raw_idle") is not True
        or report.get("watch_stopped") is not True or report.get("watch_returncode") != 0
        or report.get("native_up_dispatch_acknowledged") is not True
        or not all(report.get("watch_integrity_checks", {"unsupported": False}).values())
        or report.get("mechanical_checks", {}).get("all_requested_moves_acknowledged") is not True))
    print(json.dumps({"summary": str(args.output_dir / "summary.json"), "error": report.get("error"),
        "mechanical_checks": report.get("mechanical_checks"), "cleanup_errors": report.get("cleanup_errors"), "exit_code": code}))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
