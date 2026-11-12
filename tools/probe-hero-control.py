#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Bounded developer control probe through the exact owned runtime's socket.

No desktop cursor, settings changes, raw UI taps, or game-memory writes. Stop
other gameplay input first. Output records observations and mechanical checks;
it does not establish that an ability hit the intended target or was cancelled.

Example (NOT a read-only command):
  tools/probe-hero-control.py --hero 25 --key Q --hold-ms 600 --movement \
      --output /tmp/kagura-q-probe.json
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from runtime_identity import runtime_executable_is_owned

DIAGNOSTICS = ("input", "aim", "contacts", "cast_lock", "templates", "gestures", "cancel")
CONTROLLER_DIAGNOSTICS = ("input", "aim", "contacts", "cancel", "cast")
UNSUPPORTED_CONTROLLER = object()
WAYPOINTS = ((.60, .50), (.57, .62), (.43, .58), (.37, .45), (.48, .35), (.62, .43))
RELEASE_SECONDS = 1.8
HEARTBEAT_SECONDS = .5  # The native independent-pointer lease is two seconds.


def key_code(value: str) -> int:
    if value.upper() == "SPACE":
        return 32
    if len(value) == 1 and value.isascii() and value.isalpha():
        return ord(value.upper())
    try:
        code = int(value, 0)
    except ValueError as error:
        raise argparse.ArgumentTypeError("Use Q/W/E/R/F, SPACE, or a numeric key code") from error
    if not 1 <= code <= 0x01FFFFFF:
        raise argparse.ArgumentTypeError("Expected a keyboard key code in 1..0x01ffffff")
    return code


def bounded_int(low: int, high: int):
    def parse(value: str) -> int:
        try:
            number = int(value)
        except ValueError as error:
            raise argparse.ArgumentTypeError("Expected an integer") from error
        if not low <= number <= high:
            raise argparse.ArgumentTypeError(f"Expected {low}..{high}")
        return number
    return parse


def release_state(snapshot: dict, movement: bool) -> dict:
    """Separate an intentionally held movement finger from skill contacts."""
    data = snapshot.get("data", {})
    inputs = data.get("input", {})
    contacts = data.get("contacts", {}).get("active")
    movement_id = inputs.get("movement_touch", -1) if movement else -1
    valid = isinstance(contacts, list) and all(type(item) is int and 0 <= item < 16 for item in contacts)
    extra = [item for item in contacts if item != movement_id] if valid else None
    no_pending = inputs.get("pending_release") in (0, False)
    core_up = type(inputs.get("touch")) is int and inputs["touch"] < 0
    return {"contacts_valid": valid, "movement_contact": movement_id,
            "skill_contacts": extra, "pending_release_clear": no_pending,
            "core_touch_released": core_up,
            "released": valid and not extra and no_pending and core_up}


def observations(samples: list[dict], movement: bool, physical_up_ns: int | None) -> dict:
    held = [s for s in samples if s.get("stage", "").startswith("held:")]
    contact_seen = any(release_state(s, movement)["skill_contacts"] for s in held)
    before_up = [s for s in held if physical_up_ns is None or s["end_ns"] < physical_up_ns]
    quiet_held = any(release_state(s, movement)["released"] for s in before_up)
    seen_contact = False
    auto_release = False
    for sample in before_up:
        state = release_state(sample, movement)
        auto_release |= seen_contact and state["released"]
        seen_contact |= bool(state["skill_contacts"])
    feedback = any(any(s.get("data", {}).get("input", {}).get(field) in (1, True)
                       for field in ("accepted", "direction_accepted", "source_aim")) for s in held)
    return {"skill_contact_observed_while_held": contact_seen,
            "aim_feedback_observed_while_held": feedback,
            "no_skill_contact_observed_while_key_held": quiet_held,
            "release_before_physical_up_observed": auto_release,
            "interpretation": "Tap/instant skills may release while held; missing feedback is not proof of failure."
            }


class Probe:
    def __init__(self, expected_hero: int, report: dict, expected_pid: int | None = None):
        self.hero = expected_hero
        self.report = report
        self.metadata_path = ROOT / "logs/current-atl-pid.json"
        self.pid = json.loads(self.metadata_path.read_text())["pid"]
        if expected_pid is not None and self.pid != expected_pid:
            raise RuntimeError(f"Expected watched runtime PID {expected_pid}, selected PID is {self.pid}; refusing input")
        if type(self.pid) is not int or self.pid <= 0 or not runtime_executable_is_owned(self.pid, ROOT):
            raise RuntimeError("Recorded process is not the verified owned runtime")
        self.proc = Path("/proc") / str(self.pid)
        self.executable = (self.proc / "exe").resolve(strict=True)
        self.birth = self.read_birth()
        self.binary_stat = self.stat_binary()
        with self.executable.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        if self.stat_binary() != self.binary_stat:
            raise RuntimeError("Runtime binary changed during hashing")
        self.selection = (ROOT / ".clbb-local-runtime.json").read_bytes() if (ROOT / ".clbb-local-runtime.json").exists() else None
        self.socket_path = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")) / "clbb" / f"mapper-{self.pid}.sock"
        self.pointer = WAYPOINTS[0]
        self.last_input_at = time.monotonic()
        self.in_control = False
        self.controller_supported = None
        self.report["identity"] = {"pid": self.pid, "birth_ticks": self.birth,
            "executable": str(self.executable), "executable_sha256": digest,
            "owned_runtime_verified": True}

    def read_birth(self) -> int:
        fields = (self.proc / "stat").read_text().rsplit(")", 1)[1].split()
        if fields[0] in ("Z", "X", "x"):
            raise RuntimeError("Runtime exited")
        return int(fields[19])

    def stat_binary(self):
        stat = self.executable.stat()
        return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns

    def identity(self):
        if self.read_birth() != self.birth or (self.proc / "exe").resolve(strict=True) != self.executable:
            raise RuntimeError("Runtime PID was reused or its executable changed")
        if self.stat_binary() != self.binary_stat:
            raise RuntimeError("Verified runtime binary changed")
        if json.loads(self.metadata_path.read_text())["pid"] != self.pid:
            raise RuntimeError("Selected runtime process changed")
        marker = ROOT / ".clbb-local-runtime.json"
        if (marker.read_bytes() if marker.exists() else None) != self.selection:
            raise RuntimeError("Selected runtime candidate changed")

    def command(self, command: str, deadline: float | None = None, cleanup=False) -> str:
        self.identity()
        command_deadline = min(time.monotonic() + 1., deadline if deadline is not None else float("inf"))
        if not cleanup and (command.startswith(("pointer ", "key ", "mouse ")) or command == "begin"):
            current = json.loads(self.command("read hero.id", deadline=command_deadline))
            if current != self.hero:
                raise RuntimeError(f"Expected hero {self.hero}, current hero is {current}; refusing input")
        entry = {"command": command, "start_ns": time.monotonic_ns()}
        self.report["commands"].append(entry)
        try:
            with socket.socket(socket.AF_UNIX) as connection:
                remaining = command_deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError("Probe command deadline expired")
                connection.settimeout(remaining)
                connection.connect(str(self.socket_path))
                connection.sendall(command.encode("ascii"))
                connection.shutdown(socket.SHUT_WR)
                chunks, size = [], 0
                while True:
                    remaining = command_deadline - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError("Probe command deadline expired")
                    connection.settimeout(remaining)
                    chunk = connection.recv(8192)
                    if not chunk:
                        break
                    size += len(chunk)
                    if size > 262144:
                        raise RuntimeError("Diagnostic reply exceeds 256 KiB")
                    chunks.append(chunk)
            reply = b"".join(chunks).decode("utf-8").strip()
            entry["reply_bytes"] = size
            if not command.startswith("read "):
                entry["reply"] = reply
            if not command.startswith("read ") and not reply.startswith("OK"):
                raise RuntimeError(f"Command rejected: {command}: {reply}")
            if command.startswith(("pointer ", "key ", "mouse ")) or command == "begin":
                self.last_input_at = time.monotonic()
            if command == "begin":
                self.in_control = True
            elif command == "end":
                self.in_control = False
            entry["ok"] = True
            return reply
        except Exception as error:
            entry["ok"] = False
            entry["error"] = str(error)
            raise
        finally:
            entry["end_ns"] = time.monotonic_ns()

    def read(self, key: str, deadline=None):
        reply=self.command("read " + key, deadline=deadline)
        if key == "diagnostics.controller" and not reply:
            return UNSUPPORTED_CONTROLLER
        if key == "diagnostics.cancel" and not reply:
            return {"available":False,"reason":"unsupported_diagnostic"}
        return json.loads(reply)

    def controller_snapshot(self, deadline=None):
        """Only an explicitly unsupported endpoint permits separate legacy reads.

        A malformed/foreign combined reply or timeout is a probe failure, never
        silently relabelled as coherent telemetry from another set of RPCs.
        """
        if getattr(self, "controller_supported", None) is False:
            return None
        payload = self.read("diagnostics.controller", deadline=deadline)
        if payload is UNSUPPORTED_CONTROLLER:
            self.controller_supported = False
            return None
        if (not isinstance(payload, dict) or payload.get("atomic") is not True
                or type(payload.get("schema")) is not int or payload["schema"] != 1
                or type(payload.get("snapshot_ns")) is not int or payload["snapshot_ns"] <= 0
                or any(not isinstance(payload.get(name), dict) for name in CONTROLLER_DIAGNOSTICS)
                or not isinstance(payload.get("source"), dict)):
            raise RuntimeError("Invalid atomic controller diagnostic schema")
        inputs, aim, cast = payload["input"], payload["aim"], payload["cast"]
        if type(payload.get("hero")) is not int or payload["hero"] != self.hero or type(inputs.get("hero")) is not int or inputs["hero"] != self.hero:
            raise RuntimeError(f"Expected hero {self.hero}, atomic controller hero changed")
        if not isinstance(payload.get("epoch"), str) or not payload["epoch"] or inputs.get("epoch") != payload["epoch"]:
            raise RuntimeError("Atomic controller epoch is missing or inconsistent")
        for field in ("skill", "touch"):
            if type(inputs.get(field)) is not int or type(aim.get(field)) is not int or inputs[field] != aim[field]:
                raise RuntimeError(f"Atomic input/aim {field} is missing or inconsistent")
        if type(cast.get("touch")) is not int or cast["touch"] != inputs["touch"]:
            raise RuntimeError("Atomic cast/input touch is inconsistent")
        for field in ("direction_accepted", "direction_fresh", "direction_matched_seq"):
            if type(inputs.get(field)) is not int or type(aim.get(field)) is not int or inputs[field] != aim[field]:
                raise RuntimeError(f"Atomic Direction {field} is missing or inconsistent")
        self.controller_supported = True
        return payload

    def snapshot(self, stage: str, deadline=None, goal=None) -> dict:
        sample = {"stage": stage, "start_ns": time.monotonic_ns(), "atomic": False, "data": {}}
        if goal is not None:
            sample["pointer_goal"] = dict(goal)
        self.report["samples"].append(sample)
        try:
            if self.in_control and time.monotonic() - self.last_input_at >= HEARTBEAT_SECONDS:
                self.send_pointer(self.pointer, deadline=deadline)
            controller = self.controller_snapshot(deadline=deadline)
            if controller is not None:
                sample["atomic"] = True
                sample["controller"] = {name: controller[name] for name in
                                        ("schema", "snapshot_ns", "hero", "epoch", "source")}
                sample["data"].update({name: controller[name] for name in CONTROLLER_DIAGNOSTICS})
            else:
                sample["controller"] = {"available": False, "reason": "unsupported_diagnostic"}
            for name in DIAGNOSTICS:
                if name in sample["data"]:
                    continue
                if self.in_control and time.monotonic() - self.last_input_at >= HEARTBEAT_SECONDS:
                    self.send_pointer(self.pointer, deadline=deadline)
                sample["data"][name] = self.read("diagnostics." + name, deadline=deadline)
                if name == "input" and sample["data"][name].get("hero") != self.hero:
                    raise RuntimeError(f"Expected hero {self.hero}, current hero is {sample['data'][name].get('hero')}")
            for name in ("cast_lock", "templates"):
                observed = sample["data"][name].get("hero")
                if observed is not None and observed != self.hero:
                    raise RuntimeError(f"Hero changed in {name} diagnostics: {observed}")
            return sample
        finally:
            sample["end_ns"] = time.monotonic_ns()

    def send_pointer(self, pointer, deadline=None):
        self.pointer = pointer
        self.command(f"pointer {pointer[0]:.6f} {pointer[1]:.6f}", deadline=deadline)

    def wait_until(self, deadline: float):
        while time.monotonic() < deadline:
            now = time.monotonic()
            heartbeat_at = self.last_input_at + HEARTBEAT_SECONDS
            if now >= heartbeat_at:
                self.send_pointer(self.pointer, deadline=deadline)
            else:
                time.sleep(max(0., min(deadline, heartbeat_at) - now))


def settle_deadline(now: float, hold_deadline: float, next_waypoint: float,
                    interval: float, settle_ms: int) -> float:
    return min(hold_deadline, next_waypoint, now + min(settle_ms / 1000., interval / 3.))


def held_snapshot(probe, stage, goal, deadline):
    if deadline - time.monotonic() <= 1e-9:
        return None
    try:
        return probe.snapshot(stage, deadline=deadline, goal=goal)
    except TimeoutError:
        if time.monotonic() < deadline:
            raise
        # Reaching the hold budget is not permission to postpone physical UP.
        samples = probe.report.get("samples", [])
        if samples and samples[-1].get("stage") == stage:
            samples[-1]["truncated_at_hold_deadline"] = True
        return None


def hold_waypoints(probe, report, down_at: float, hold_ms: int, settle_ms: int):
    deadline = down_at + hold_ms / 1000.
    interval = hold_ms / 1000. / len(WAYPOINTS)
    report["hold_deadline_ns"] = round(deadline * 1e9)
    for index, pointer in enumerate(WAYPOINTS):
        probe.wait_until(down_at + interval * index)
        if deadline - time.monotonic() <= 1e-9:
            break
        try:
            probe.send_pointer(pointer, deadline=deadline)
        except TimeoutError:
            if time.monotonic() >= deadline:
                break
            raise
        command = report["commands"][-1]
        goal = {"index": index, "pointer_uv": list(pointer),
                "command_start_ns": command["start_ns"], "command_end_ns": command["end_ns"]}
        held_snapshot(probe, f"held:{index}", goal, deadline)
        if settle_ms and time.monotonic() < deadline:
            now = time.monotonic()
            target = settle_deadline(now, deadline, down_at + interval * (index + 1), interval, settle_ms)
            if target > now:
                goal["settle_wait_ms"] = (target - now) * 1000.
                probe.wait_until(target)
                held_snapshot(probe, f"held:{index}:settled", goal, deadline)
    probe.wait_until(deadline)


def run(args, report: dict):
    probe = Probe(args.hero, report, getattr(args, "expected_pid", None))
    initial = probe.snapshot("preflight")
    if initial["data"]["contacts"].get("active") != []:
        raise RuntimeError("Initial contacts are not empty; refusing to release somebody else's gesture")
    inputs = initial["data"]["input"]
    if not inputs.get("ready") or not inputs.get("enabled"):
        raise RuntimeError("Mapper/game profile is not ready and enabled")
    keys = {args.key}
    cancel_key = key_code(probe.command("read hero.cancel_key")) if args.cancel else None
    if cancel_key is not None:
        if cancel_key == args.key:
            raise RuntimeError("Skill and cancel keys are identical")
        keys.add(cancel_key)
    hotkeys = {key_code(probe.command("read " + name)) for name in
               ("hotkeys.settings", "hotkeys.confine", "hotkeys.fullscreen")}
    if keys & hotkeys:
        raise RuntimeError("Requested key overlaps a host settings/fullscreen/cursor shortcut")
    movement_ms = 0
    if args.movement:
        if int(probe.command("read movement.button")) != 0x02000002:
            raise RuntimeError("--movement requires the existing movement binding to be RMB")
        movement_ms = bounded_int(40, 600)(probe.command("read movement.hold_ms")) + 25
    report["initial_checks"] = {"expected_hero": True, "contacts_empty": True, "mapper_ready": True}
    report["cancel_key"] = cancel_key
    began = False
    pressed_keys = set()
    movement_attempted = False
    physical_up_ns = None
    pre_end = None
    try:
        # Mark before sending: a lost reply must still lead to cleanup.
        began = True
        probe.command("begin")
        probe.send_pointer(WAYPOINTS[0])
        if args.movement:
            movement_attempted = True
            probe.command("mouse 2 1")
            probe.wait_until(time.monotonic() + movement_ms / 1000.)
            probe.snapshot("movement_held")
        pressed_keys.add(args.key)
        down_at = time.monotonic()
        down_reply = probe.command(f"key {args.key} 1")
        report["skill_down_ns"] = report["commands"][-1]["start_ns"]
        hold_waypoints(probe, report, down_at, args.hold_ms, getattr(args, "settle_ms", 0))
        if args.cancel:
            pressed_keys.add(cancel_key)
            cancel_reply = probe.command(f"key {cancel_key} 1")
            probe.snapshot("cancel_requested")
            probe.command(f"key {cancel_key} 0")
        physical_up_ns = time.monotonic_ns()
        probe.command(f"key {args.key} 0")
        report["physical_up_ns"] = physical_up_ns
        deadline = time.monotonic() + RELEASE_SECONDS
        while True:
            pre_end = probe.snapshot("release_poll", deadline=deadline)
            if release_state(pre_end, args.movement)["released"] or time.monotonic() >= deadline:
                break
            probe.wait_until(min(deadline, time.monotonic() + .05))
        report["release_before_forced_end"] = release_state(pre_end, args.movement)
        report["observations"] = observations(report["samples"], args.movement, physical_up_ns)
        report["mechanical_checks"] = {
            "skill_down_handled": "handled=1" in down_reply,
            "cancel_command_handled": "handled=1" in cancel_reply if args.cancel else None,
            "no_skill_contact_before_forced_end": report["release_before_forced_end"]["contacts_valid"] and not report["release_before_forced_end"]["skill_contacts"],
            "pending_release_cleared_before_forced_end": report["release_before_forced_end"]["pending_release_clear"],
            "release_complete_before_forced_end": report["release_before_forced_end"]["released"],
            "movement_hold_observed": any(s["data"].get("input", {}).get("movement_held") in (1, True)
                                           for s in report["samples"] if s["stage"] == "movement_held") if args.movement else None,
        }
    finally:
        cleanup_errors = []
        if began and any(entry["command"] == "begin" for entry in report["commands"]):
            # Never omit UP because a DOWN/UP reply was lost. Identity checks
            # still forbid sending input into a reused PID or replaced runtime.
            for code in sorted(pressed_keys):
                try:
                    probe.command(f"key {code} 0", cleanup=True)
                except Exception as error:
                    cleanup_errors.append(str(error))
            if movement_attempted:
                try:
                    probe.command("mouse 2 0", cleanup=True)
                except Exception as error:
                    cleanup_errors.append(str(error))
            try:
                probe.command("end", cleanup=True)
            except Exception as error:
                cleanup_errors.append(str(error))
            try:
                final = probe.snapshot("after_forced_end")
                report["cleanup_contacts_empty"] = final["data"]["contacts"].get("active") == []
            except Exception as error:
                cleanup_errors.append(str(error))
        report["cleanup_errors"] = cleanup_errors


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--hero", type=bounded_int(1, 10000), required=True, help="Expected current hero ID; never selects a hero")
    parser.add_argument("--expected-pid", type=bounded_int(1, 0x7fffffff), help="Refuse input if the selected runtime changed after a native watcher started")
    parser.add_argument("--key", type=key_code, required=True, help="Keyboard bind, e.g. Q, W, E, SPACE, or decimal code 81")
    parser.add_argument("--hold-ms", type=bounded_int(16, 5000), default=600)
    parser.add_argument("--settle-ms", type=bounded_int(0, 250), default=0, help="Diagnostic follow-up sample delay, capped by waypoint/hold budgets; does not change mapper latency")
    parser.add_argument("--movement", action="store_true", help="Hold the existing RMB movement bind concurrently")
    parser.add_argument("--cancel", action="store_true", help="Press the existing hero cancel bind before physical key UP")
    parser.add_argument("--output", type=Path, required=True, help="A new JSON file; never overwrites an earlier probe")
    args = parser.parse_args(argv)
    report = {"format": "clbb-hero-control-probe", "version": 1, "start_ns": time.monotonic_ns(),
              "request": {"hero": args.hero, "key": args.key, "hold_ms": args.hold_ms,
                          "movement": args.movement, "cancel": args.cancel, "settle_ms": args.settle_ms},
              "commands": [], "samples": [], "status": "observation_only",
              "release_mode": "explicit_cancel" if args.cancel else "physical_up",
              "limits": "Telemetry probe only; hit accuracy, native cast success and cancellation require gameplay/result evidence."}
    try:
        # Reserve the report before any game input, and refuse overwrites.
        with args.output.open("x", encoding="utf-8") as output:
            try:
                run(args, report)
            except (Exception, KeyboardInterrupt) as error:
                report["error"] = f"{type(error).__name__}: {error}"
            finally:
                report["end_ns"] = time.monotonic_ns()
                json.dump(report, output, ensure_ascii=False, indent=2)
                output.write("\n")
    except OSError as error:
        parser.error(f"Cannot create a new report: {error}")
    print(json.dumps({"report": str(args.output.resolve()), "error": report.get("error"),
                      "mechanical_checks": report.get("mechanical_checks"),
                      "cleanup_contacts_empty": report.get("cleanup_contacts_empty")}, ensure_ascii=False))
    checks = report.get("mechanical_checks", {})
    return int(bool(report.get("error") or report.get("cleanup_errors")
                    or report.get("cleanup_contacts_empty") is not True
                    or any(value is False for value in checks.values())))


if __name__ == "__main__":
    raise SystemExit(main())
