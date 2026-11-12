#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Root coordinator's serial training visit/audit/probe workflow.

Even without --basic-probes this changes heroes, prepares their training fixture
and teleports them to the verified river center. It never spawns bots, changes
persisted CLBB settings, restarts a runtime, resumes a failed batch, or declares
gameplay correct from process exits/counters. Run only while the coordinator owns
all gameplay input; native read-only UI audits normally require sudo -n.
"""
from __future__ import annotations
import argparse
import contextlib
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from runtime_identity import runtime_executable_is_owned


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


chooser = load("visit_chooser", ROOT / "tools/choose-training-hero.py")
setup_tool = load("visit_setup", ROOT / "tools/setup-training-hero.py")
river = load("visit_river", ROOT / "tools/setup-training-river.py")
probe_tool = load("visit_probe", ROOT / "tools/probe-training-skill.py")


class BatchAbort(RuntimeError):
    pass


def parse_heroes(value):
    parts = value.split(",")
    if not 1 <= len(parts) <= 64 or any(not part.strip().isascii() or not part.strip().isdigit() for part in parts):
        raise argparse.ArgumentTypeError("Use one to64 comma-separated native hero IDs")
    heroes = [int(part.strip()) for part in parts]
    if any(not 1 <= hero <= 9999 for hero in heroes) or len(set(heroes)) != len(heroes):
        raise argparse.ArgumentTypeError("Hero IDs must be unique and in1..9999")
    return heroes


def validate_idle_state(state, expected_hero=None):
    hero = state.get("hero")
    if type(hero) is not int or not 1 <= hero <= 9999 or (expected_hero is not None and hero != expected_hero):
        raise BatchAbort(f"Unexpected hero: expected {expected_hero}, observed {hero}")
    setup_tool.validate_controls(state.get("controls"), hero)
    if state.get("contacts", {}).get("active") != []:
        raise BatchAbort("Mapped contacts are held or unknown; refusing the next phase")
    if state.get("delivery", {}).get("raw_gesture_active") is not False:
        raise BatchAbort("Raw game gesture is active or unknown; refusing the next phase")
    inputs = state.get("input", {})
    if inputs.get("hero") != hero or inputs.get("ready") not in (1, True) or inputs.get("enabled") not in (1, True):
        raise BatchAbort("Current mapped profile is not ready/enabled for this hero")
    if type(inputs.get("touch")) is not int or inputs["touch"] >= 0 or inputs.get("pending_release") not in (0, False):
        raise BatchAbort("A skill contact/release remains active or unknown")
    if state.get("gestures", {}).get("active") != []:
        raise BatchAbort("Linked gestures remain active or unknown")
    return hero


def plan_probes(audit, controls):
    """Use actual current button/variant contracts, never hero-ID arithmetic."""
    if audit.get("hero") != controls.get("hero"):
        raise BatchAbort("Audit hero does not match current native controls")
    mapped = {row.get("slot"): row for row in controls.get("buttons", [])}
    variants = {}
    for row in audit.get("variants", []):
        slot = row.get("slot")
        if slot in variants:
            raise BatchAbort(f"Ambiguous native variant: {slot}")
        variants[slot] = row
    result, seen = [], set()
    keys = {"skill:0": 81, "skill:1": 87, "skill:2": 69, "skill:3": 82}
    for button in audit.get("buttons", []):
        if button.get("kind") != "skill":
            continue
        slot = button.get("slot")
        if slot in seen:
            raise BatchAbort(f"Ambiguous native skill button: {slot}")
        seen.add(slot)
        base = button.get("base_skill")
        variant = variants.get(slot)
        skill = variant.get("active") if variant is not None else base
        kind = audit.get("types", {}).get(slot, button.get("template"))
        row = {"slot": slot, "base_skill": base, "skill": skill, "template": kind,
               "key": keys.get(slot), "hold_ms": 300 if type(kind) is int and kind == 1 else 650 if type(kind) is int and kind in (2, 3) else None,
               "skip_reason": None}
        current = mapped.get(slot)
        if slot not in keys:
            row["skip_reason"] = "native_skill_slot_has_no_standard_QWER_probe"
        elif type(base) is not int or not 1 <= base <= 10_000_000 or type(skill) is not int or not 1 <= skill <= 10_000_000:
            row["skip_reason"] = "native_skill_identity_unknown"
        elif variant is not None and variant.get("base") != base:
            row["skip_reason"] = "native_variant_does_not_belong_to_button"
        elif type(kind) is not int or kind not in (1, 2, 3):
            row["skip_reason"] = "native_template_unknown"
        elif button.get("active") is not True:
            row["skip_reason"] = "native_button_unavailable"
        elif not isinstance(current, dict) or current.get("available") is not True or current.get("skill_id") != base:
            row["skip_reason"] = "current_mapped_button_unavailable_or_changed"
        elif current.get("key") != keys[slot]:
            row["skip_reason"] = "existing_binding_is_not_the_standard_QWER_key"
        result.append(row)
    return sorted(result, key=lambda row: str(row["slot"]))


def probe_plan_status(plan, state, cast_lock):
    """A previous audit never authorizes a missing/changed current button."""
    controls = state.get("controls", {})
    rows = [row for row in controls.get("buttons", []) if row.get("slot") == plan.get("slot")]
    if len(rows) != 1:
        return "current_mapped_slot_missing_or_ambiguous"
    current = rows[0]
    if (current.get("kind") != "skill" or current.get("available") is not True
            or current.get("skill_id") != plan.get("base_skill") or current.get("key") != plan.get("key")):
        return "current_mapped_contract_changed"
    if controls.get("types", {}).get(plan.get("slot")) != plan.get("template"):
        return "current_native_template_changed_or_unknown"
    if not isinstance(cast_lock, dict) or cast_lock.get("hero") != state.get("hero") or cast_lock.get("epoch") != controls.get("epoch"):
        return "current_variant_owner_unknown"
    start, end = cast_lock.get("start_ns"), cast_lock.get("end_ns")
    now = time.monotonic_ns()
    if (any(type(value) not in (int, float) or not math.isfinite(value) for value in (start, end))
            or start < 0 or end < start or end - start > 100_000_000 or end > now + 10_000_000 or now - end > 100_000_000):
        return "current_variant_clock_unknown_or_stale"
    rows = cast_lock.get("buttons")
    if not isinstance(rows, list) or len(rows) > 32:
        return "current_variant_rows_unknown"
    current = [row for row in rows if row.get("slot") == plan.get("slot")]
    if len(current) != 1 or type(current[0].get("form")) is not int or not 1 <= current[0]["form"] <= 10_000_000:
        return "current_variant_missing_or_unknown"
    if current[0]["form"] != plan.get("skill"):
        return "current_variant_changed"
    return None


class RuntimeAnchor:
    def __init__(self):
        self.runtime = chooser.OwnedRuntime()
        self.pid, self.birth, self.executable = self.runtime.pid, str(self.runtime.birth), self.runtime.executable
        self.reader = setup_tool.probe_module.Probe(1, {"commands": [], "samples": []}, expected_pid=self.pid)
        if not self.reader.socket_path.exists():
            self.reader.socket_path = Path(f"/run/user/{self.reader.proc.stat().st_uid}/clbb/mapper-{self.pid}.sock")
        self.match_session(self.reader)
    def identity(self):
        self.runtime.identity()
        self.reader.identity()
    def proof(self):
        self.identity()
        if not runtime_executable_is_owned(self.pid, ROOT):
            raise BatchAbort("Selected owned runtime proof changed; batch will not restart or retarget")
    def match_session(self, session):
        self.identity()
        if session.pid != self.pid or str(session.birth) != self.birth or Path(session.executable) != self.executable:
            raise BatchAbort("Helper attached to a different runtime PID/birth/executable")
    def selected_runtime(self):
        self.proof()
        return self.pid, self.reader.proc.stat().st_uid
    def preflight(self, expected=None):
        self.proof()
        state = {"ns": time.monotonic_ns(), "hero": self.reader.read("hero.id")}
        for name in ("controls", "contacts", "delivery", "input", "gestures"):
            state[name] = self.reader.read("diagnostics." + name)
        validate_idle_state(state, expected)
        self.reader.report["commands"].clear()  # Read-only guard traffic is summarized by state receipts.
        return state
    def describe(self):
        return {"pid": self.pid, "birth_ticks": self.birth, "executable": str(self.executable), "owned_runtime_verified": True}


class Coordinator:
    def __init__(self, args, report, journal, anchor=None):
        self.args, self.report, self.journal = args, report, journal
        self.anchor = anchor if anchor is not None else RuntimeAnchor()
        self.current_hero = None
        self.hero_report = None
    def emit(self, row):
        print(json.dumps(row, ensure_ascii=False), flush=True)
        self.journal.write(json.dumps(row, ensure_ascii=False) + "\n")
        self.journal.flush()
    def phase(self, hero, name, callback, expected_before, expected_after, allow_epoch_change=False):
        row = {"hero": hero, "phase": name, "start_ns": time.monotonic_ns()}
        self.hero_report["phases"].append(row)
        self.emit({"event": "phase_start", **row})
        try:
            row["before"] = self.anchor.preflight(expected_before)
            result = callback()
            row["after"] = self.anchor.preflight(expected_after)
            if not allow_epoch_change and row["before"]["controls"]["epoch"] != row["after"]["controls"]["epoch"]:
                raise BatchAbort("Unexpected local Actor epoch change inside a phase")
            row["workflow_phase_finished"] = True
            return result
        except BaseException as error:
            row["error"] = f"{type(error).__name__}: {error}"
            try:
                row["state_after_error"] = self.anchor.preflight(expected_after)
            except Exception as check_error:
                row["state_after_error_failed"] = f"{type(check_error).__name__}: {check_error}"
            raise
        finally:
            row["end_ns"] = time.monotonic_ns()
            self.emit({"event": "phase_end", "hero": hero, "phase": name, "error": row.get("error"), "end_ns": row["end_ns"]})
    def guarded_setup_class(self):
        anchor, base = self.anchor, setup_tool.Setup
        class AnchoredSetup(base):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, **kwargs)
                anchor.match_session(self)
            def identity(self):
                super().identity()
                anchor.match_session(self)
        return AnchoredSetup
    def choose(self, hero, directory):
        selector = chooser.Selector(self.anchor.runtime, directory / "choose")
        selector.open()  # Same observed wrench/picker path as --open-picker.
        selector.choose(hero, 20)
        selector.confirm(hero)
    def setup(self, hero, directory):
        path = directory / "setup.json"
        original = setup_tool.Setup
        setup_tool.Setup = self.guarded_setup_class()
        try:
            with (directory / "setup.stdout.txt").open("x") as stdout, (directory / "setup.stderr.txt").open("x") as stderr, contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                code = setup_tool.main(["--hero", str(hero), "--max-level", "--configure-flags", "--verify-flags",
                    "--gm-close-point", *map(str, self.args.gm_close_point), "--output", str(path)])
        finally:
            setup_tool.Setup = original
        result = json.loads(path.read_text())
        if code or result.get("error") or result.get("cleanup_errors") or result.get("cleanup_contacts_empty") is not True:
            raise BatchAbort(f"Training setup/cleanup failed; inspect {path}")
        if result.get("learning", {}).get("available_upgrades_exhausted") is not True:
            raise BatchAbort(f"Training upgrades are not exhausted; inspect {path}")
        if result.get("max_level", {}).get("level_15_verified") is not True:
            raise BatchAbort(f"Native hero level15 was not independently verified; inspect {path}")
        self.hero_report["setup"] = {"path": str(path), "max_level": result.get("max_level"), "gm_flags": result.get("gm_flags"), "learning": result.get("learning")}
        return result
    def audit(self, hero, directory, name="contract"):
        path = directory / f"{name}.json"
        binary = ROOT / "mapper/runtime/target/debug/hero-contract-audit"
        size = [self.anchor.reader.read("display.width"), self.anchor.reader.read("display.height")]
        if any(type(value) is not int or not 32 <= value <= 16384 for value in size):
            raise BatchAbort("Native audit resolution is unknown")
        with (directory / f"{name}.stdout.txt").open("x") as stdout, (directory / f"{name}.stderr.txt").open("x") as stderr:
            # This child is read-only. Input helpers remain in their own bounded
            # cleanup paths and are never SIGKILLed by the batch coordinator.
            subprocess.run([str(binary), str(self.anchor.pid), str(path), *map(str, size)], cwd=ROOT,
                           stdout=stdout, stderr=stderr, check=True, timeout=30)
        result = json.loads(path.read_text())
        if (result.get("pid") != self.anchor.pid or str(result.get("birth")) != self.anchor.birth
                or result.get("hero") != hero or result.get("same_match_at_end") is not True):
            raise BatchAbort(f"Native audit identity/match was not stable; inspect {path}")
        setup_tool.validate_hero_level(result, hero, self.anchor.pid, self.anchor.birth,
                                       expected_executable=self.anchor.executable)
        self.hero_report["audit_paths"].append(str(path))
        return result
    def probe(self, hero, plan, directory):
        current = self.anchor.preflight(hero)
        reason = probe_plan_status(plan, current, self.anchor.reader.read("diagnostics.cast_lock"))
        if reason:
            raise BatchAbort(f"Probe contract became unconfirmed before child start: {reason}")
        output = directory / (str(plan["slot"]).replace(":", "-") + "-probe")
        original = probe_tool.selected_runtime
        probe_tool.selected_runtime = self.anchor.selected_runtime
        try:
            with (directory / (output.name + ".stdout.txt")).open("x") as stdout, (directory / (output.name + ".stderr.txt")).open("x") as stderr, contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                code = probe_tool.main(["--hero", str(hero), "--key", str(plan["key"]), "--skill", str(plan["skill"]),
                    "--hold-ms", str(plan["hold_ms"]), "--movement", "--settle-ms", "30", "--output-dir", str(output)])
        finally:
            probe_tool.selected_runtime = original
        summary = json.loads((output / "summary.json").read_text())
        evidence = {"slot": plan["slot"], "skill": plan["skill"], "path": str(output / "summary.json"),
                    "mechanical_checks": summary.get("mechanical_checks"), "actor_counter_evidence": summary.get("actor_counter_evidence"),
                    "watch_integrity_checks": summary.get("watch_integrity_checks"), "probe_error": summary.get("probe_error"), "error": summary.get("error")}
        self.hero_report["probes"].append(evidence)
        checks = summary.get("mechanical_checks")
        if (code or summary.get("error") or summary.get("probe_error") or summary.get("cleanup_errors")
                or summary.get("cleanup_contacts_empty") is not True or summary.get("watch_stopped") is not True
                or not isinstance(checks, dict) or not checks or any(value is False for value in checks.values())
                or not all((summary.get("watch_integrity_checks") or {"missing": False}).values())):
            raise BatchAbort(f"Probe or its cleanup/integrity failed; batch stops: {output}")
        return evidence
    def return_center(self, hero, directory):
        path = directory / "return-center.json"
        result = {"format": "clbb-training-center-return", "hero": hero, "status": "observation_only", "commands": [], "samples": [],
                  "phases": [], "pressed_keys": [], "map_clicks": [], "world_receipts": [], "spawn_requests": []}
        with path.open("x") as output:
            session = None
            try:
                session = self.guarded_setup_class()(hero, result, directory / "return-center.phases")
                session.expected_hero()
                session.controls("preflight")
                session.raw_idle()
                result["world_receipt"] = river.teleport(session, river.CENTER[0], river.CENTER[1], .9, "return-center")
            except BaseException as error:
                result["error"] = f"{type(error).__name__}: {error}"
                raise
            finally:
                errors = []
                if session is not None:
                    try:
                        session.raw_idle()
                        session.command("end", cleanup=True)
                    except Exception as error:
                        errors.append(str(error))
                    try:
                        result["cleanup_contacts_empty"] = session.read("diagnostics.contacts").get("active") == []
                    except Exception as error:
                        errors.append(str(error))
                result["cleanup_errors"] = errors
                result["end_ns"] = time.monotonic_ns()
                json.dump(result, output, ensure_ascii=False, indent=2)
                output.write("\n")
        if result.get("cleanup_errors") or result.get("cleanup_contacts_empty") is not True or not result.get("world_receipt"):
            raise BatchAbort(f"Center teleport or cleanup was not confirmed: {path}")
        self.hero_report["center_return"] = {"path": str(path), "world_receipt": result["world_receipt"], "spawn_requests": 0}
    def run(self):
        self.report["identity"] = self.anchor.describe()
        self.current_hero = self.anchor.preflight()["hero"]
        for hero in self.args.heroes:
            directory = self.args.output_dir / f"hero-{hero:04d}"
            directory.mkdir(exist_ok=False)
            self.hero_report = {"hero": hero, "status": "observation_only", "phases": [], "audit_paths": [], "probes": [], "skipped": []}
            brief = {"hero": hero, "path": str(directory / "report.json"), "workflow_finished": False}
            self.report["heroes"].append(brief)
            with (directory / "report.json").open("x") as output:
                try:
                    self.phase(hero, "choose", lambda: self.choose(hero, directory), self.current_hero, hero, allow_epoch_change=True)
                    self.current_hero = hero
                    self.phase(hero, "setup", lambda: self.setup(hero, directory), hero, hero)
                    audit = self.phase(hero, "contract", lambda: self.audit(hero, directory), hero, hero)
                    controls = self.anchor.preflight(hero)["controls"]
                    plans = plan_probes(audit, controls)
                    self.hero_report["initial_probe_plan"] = plans
                    for index, plan in enumerate(plans):
                        if plan["skip_reason"]:
                            self.hero_report["skipped"].append(plan)
                            continue
                        if not self.args.basic_probes:
                            self.hero_report["skipped"].append({**plan, "skip_reason": "basic_probes_not_requested"})
                            continue
                        current = self.anchor.preflight(hero)
                        reason = probe_plan_status(plan, current, self.anchor.reader.read("diagnostics.cast_lock"))
                        if reason:
                            audit = self.phase(hero, f"contract-refresh-{index}", lambda: self.audit(hero, directory, f"contract-refresh-{index}"), hero, hero)
                            refreshed = plan_probes(audit, self.anchor.preflight(hero)["controls"])
                            plan = next((row for row in refreshed if row["slot"] == plan["slot"]), {**plan, "skip_reason": "native_slot_disappeared"})
                            if plan["skip_reason"]:
                                self.hero_report["skipped"].append(plan)
                                continue
                            current = self.anchor.preflight(hero)
                            reason = probe_plan_status(plan, current, self.anchor.reader.read("diagnostics.cast_lock"))
                            if reason:
                                self.hero_report["skipped"].append({**plan, "skip_reason": "contract_unconfirmed_after_refresh:" + reason})
                                continue
                        self.phase(hero, f"probe-{plan['slot']}", lambda plan=plan: self.probe(hero, plan, directory), hero, hero)
                    self.phase(hero, "return-center", lambda: self.return_center(hero, directory), hero, hero)
                    brief["workflow_finished"] = True
                    self.hero_report["workflow_finished"] = True
                except BaseException as error:
                    brief["error"] = self.hero_report["error"] = f"{type(error).__name__}: {error}"
                    raise
                finally:
                    self.hero_report["end_ns"] = time.monotonic_ns()
                    json.dump(self.hero_report, output, ensure_ascii=False, indent=2)
                    output.write("\n")
        self.report["sequence_finished"] = True


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--heroes", type=parse_heroes, required=True)
    parser.add_argument("--output-dir", type=Path, required=True, help="Must be a new directory; failed batches are never auto-resumed")
    parser.add_argument("--basic-probes", action="store_true", help="Additionally run serial QWER probes for observed known templates")
    parser.add_argument("--gm-close-point", type=float, nargs=2, default=(.974, .1), metavar=("U", "V"))
    args = parser.parse_args(argv)
    if any(not math.isfinite(value) or not 0 <= value <= 1 for value in args.gm_close_point):
        parser.error("GM close point must be finite normalized coordinates")
    args.output_dir = args.output_dir.resolve()
    try:
        args.output_dir.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        parser.error(f"Cannot reserve a new batch directory: {error}")
    report = {"format": "clbb-training-hero-visits", "version": 1, "status": "observation_only", "start_ns": time.monotonic_ns(),
              "request": {"heroes": args.heroes, "basic_probes": args.basic_probes, "gm_close_point": args.gm_close_point}, "heroes": [],
              "limits": "Workflow completion is not proof of hero level15, perfect controls, successful/cancelled casts or native bot count. Existing bots are reused; no spawn requests or persisted settings writes."}
    with (args.output_dir / "summary.json").open("x") as output, (args.output_dir / "events.jsonl").open("x") as journal:
        try:
            Coordinator(args, report, journal).run()
        except (Exception, KeyboardInterrupt, SystemExit) as error:
            report["error"] = f"{type(error).__name__}: {error}"
        finally:
            report["end_ns"] = time.monotonic_ns()
            json.dump(report, output, ensure_ascii=False, indent=2)
            output.write("\n")
    print(json.dumps({"summary": str(args.output_dir / "summary.json"), "error": report.get("error"),
                      "sequence_finished": report.get("sequence_finished", False), "heroes": report["heroes"]}, ensure_ascii=False), flush=True)
    return int(bool(report.get("error") or report.get("sequence_finished") is not True))


if __name__ == "__main__":
    raise SystemExit(main())
