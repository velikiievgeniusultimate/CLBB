#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Choose one training hero through owned UI/socket input, without a desktop cursor.

Developer tool, not a game API. Read every target from current native UI state,
verify the selected hero ID before confirmation, and keep evidence per step.
Use --inspect-only for a read-only picker snapshot. Only one process may drive
this runtime; stop other gameplay input before invoking this tool.
"""
from __future__ import annotations
import argparse
import ctypes as C
import json
import math
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from runtime_identity import runtime_executable_is_owned


class IOVec(C.Structure):
    _fields_ = [("base", C.c_void_p), ("length", C.c_size_t)]


class OwnedRuntime:
    def __init__(self):
        self.pid = json.loads((ROOT / "logs/current-atl-pid.json").read_text())["pid"]
        if not runtime_executable_is_owned(self.pid, ROOT):
            raise RuntimeError("Recorded executable is not the selected owned runtime")
        self.executable = (Path("/proc") / str(self.pid) / "exe").resolve(strict=True)
        self.birth = self._birth()
        self.lib = C.CDLL(None, use_errno=True)
        self.lib.process_vm_readv.argtypes = [C.c_int, C.POINTER(IOVec), C.c_ulong,
                                            C.POINTER(IOVec), C.c_ulong, C.c_ulong]
        self.lib.process_vm_readv.restype = C.c_ssize_t
        self.fields = {}

    def _birth(self):
        fields = Path(f"/proc/{self.pid}/stat").read_text().rsplit(")", 1)[1].split()
        if fields[0] in ("Z", "X", "x"):
            raise RuntimeError("Runtime has exited")
        return fields[19]

    def identity(self):
        if self._birth() != self.birth or Path(f"/proc/{self.pid}/exe").resolve(strict=True) != self.executable:
            raise RuntimeError("Runtime identity changed; refusing further input")
        if json.loads((ROOT / "logs/current-atl-pid.json").read_text())["pid"] != self.pid:
            raise RuntimeError("Selected runtime PID changed")

    def read(self, address, length):
        if not 4096 < address or not 0 <= length <= 65536:
            raise RuntimeError("Read exceeds diagnostic bounds")
        data = C.create_string_buffer(length)
        local, remote = IOVec(C.addressof(data), length), IOVec(address, length)
        size = self.lib.process_vm_readv(self.pid, C.byref(local), 1, C.byref(remote), 1, 0)
        if size != length:
            raise OSError(C.get_errno(), "Owned UI memory changed or became unreadable")
        return data.raw

    def u64(self, address):
        return struct.unpack("<Q", self.read(address, 8))[0]

    def i32(self, address):
        return struct.unpack("<i", self.read(address, 4))[0]

    def cstring(self, address):
        return self.read(address, 192).split(b"\0", 1)[0].decode("utf-8", errors="strict")

    def class_name(self, address):
        if self.u64(address + 0x40) != address or self.u64(address + 0x48) != address:
            raise RuntimeError("Native class owner changed")
        return self.cstring(self.u64(address + 16))

    def field(self, obj, name):
        owner = self.u64(obj)
        key = owner, name
        if key in self.fields:
            return obj + self.fields[key]
        current = owner
        for _ in range(20):
            self.class_name(current)
            count = struct.unpack("<H", self.read(current + 0x104, 2))[0]
            if count > 1024:
                raise RuntimeError("Native field count exceeds diagnostic bounds")
            table = self.u64(current + 0x78)
            for i in range(count):
                at = table + i * 32
                if self.cstring(self.u64(at)) != name:
                    continue
                attrs = struct.unpack("<I", self.read(self.u64(at + 8) + 8, 4))[0]
                offset = struct.unpack("<I", self.read(at + 24, 4))[0]
                if attrs & 16 or offset > 65536:
                    raise RuntimeError("Expected a bounded instance field")
                self.fields[key] = offset
                return obj + offset
            current = self.u64(current + 0x58)
            if not current:
                break
        raise RuntimeError(f"Native field is absent: {name}")

    def int_list(self, obj):
        array = self.u64(obj + 16)
        count = self.i32(obj + 24)
        if not 0 <= count <= 256 or self.u64(array + 24) < count:
            raise RuntimeError("Invalid native hero list")
        array_class = self.u64(array)
        if self.cstring(self.u64(array_class + 16)) != "Int32[]":
            raise RuntimeError("Hero list is not Int32[]")
        if self.class_name(self.u64(array_class + 0x40)) != "Int32":
            raise RuntimeError("Hero list element owner changed")
        raw = self.read(array + 32, count * 4)
        values = list(struct.unpack("<" + "i" * count, raw))
        if len(set(values)) != count or any(not 0 < v < 10000 for v in values):
            raise RuntimeError("Hero list has ambiguous or invalid IDs")
        if self.u64(obj + 16) != array or self.i32(obj + 24) != count or self.read(array + 32, count * 4) != raw:
            raise RuntimeError("Hero list changed during observation")
        return values

    def rpc(self, command):
        self.identity()
        path = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")) / "clbb" / f"mapper-{self.pid}.sock"
        # A sudo diagnostic process must use the game owner's socket directory.
        if not path.exists():
            owner = Path(f"/proc/{self.pid}").stat().st_uid
            path = Path(f"/run/user/{owner}/clbb/mapper-{self.pid}.sock")
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(3)
            connection.connect(str(path))
            connection.sendall(command.encode())
            connection.shutdown(socket.SHUT_WR)
            result = connection.recv(16384).decode().strip()
        return result

    def idle(self, timeout=3):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            state = json.loads(self.rpc("read diagnostics.delivery"))
            if state.get("raw_gesture_active") is False:
                return
            time.sleep(0.025)
        raise RuntimeError("Previous raw game gesture did not finish")

    def tap(self, point):
        self.idle()
        x, y = point
        if not 0 <= x <= 1 or not 0 <= y <= 1:
            raise RuntimeError("Native tap target is outside the game surface")
        result = self.rpc(f"tap {x:.12f} {y:.12f}")
        if result != "OK tap":
            raise RuntimeError(result)
        self.idle()
        time.sleep(0.20)

    def swipe(self, downward=False):
        self.idle()
        # Only a path through the observed hero-grid interior; no desktop input.
        y1, y2 = (0.30, 0.70) if downward else (0.70, 0.30)
        result = self.rpc(f"swipe 0.70 {y1} 0.70 {y2}")
        if result != "OK swipe":
            raise RuntimeError(result)
        self.idle()
        # Allow NGUI inertia to settle before measuring cell coordinates again.
        time.sleep(0.65)


def observed_cell_geometry(cell):
    """Use the rendered UIButton widget, never a missing/stale object anchor."""
    value = cell.get("button_geometry")
    if not isinstance(value, dict):
        return None, False
    point = value.get("center_uv")
    if not isinstance(point, (tuple, list)) or len(point) != 2 or any(
            isinstance(v, bool) or not isinstance(v, (float, int)) or not math.isfinite(v) for v in point):
        return None, False
    return point, value.get("visible") is True and value.get("onscreen") is True


# Conservative, root-observed interior of this training hero grid. A rendered
# portrait may straddle its edge while still exposing a large clickable area.
PICKER_CLIP = (.24, .21, .97, .74)


def visible_cell_press(cell):
    point, visible = observed_cell_geometry(cell)
    if point is None or not visible:
        return None
    left, top, right, bottom = PICKER_CLIP
    if left <= point[0] <= right and top <= point[1] <= bottom:
        return point
    radius = cell["button_geometry"].get("radius_uv")
    if not isinstance(radius, (list, tuple)) or len(radius) != 2 or any(
            isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) or not 0 < v < .4 for v in radius):
        return None
    x0, x1 = max(left, point[0] - radius[0]), min(right, point[0] + radius[0])
    y0, y1 = max(top, point[1] - radius[1]), min(bottom, point[1] + radius[1])
    # Do not target a one-pixel sliver. At least25% of each measured widget
    # dimension must remain within the conservative grid interior.
    if x1 - x0 < 2 * radius[0] * .25 or y1 - y0 < 2 * radius[1] * .25:
        return None
    return [(x0 + x1) / 2, (y0 + y1) / 2]


class Selector:
    def __init__(self, runtime, directory):
        self.runtime = runtime
        self.directory = directory
        directory.mkdir(parents=True, exist_ok=True)
        self.serial = 0
        self.binary = ROOT / "mapper/runtime/target/debug/training-ui-audit"
        if not self.binary.is_file():
            raise RuntimeError("Build the training-ui-audit developer binary first")

    def audit(self, name, family=None, timeout=20):
        self.runtime.identity()
        self.serial += 1
        output = self.directory / f"{self.serial:03d}-{name}.ui.json"
        args = [str(self.binary), str(self.runtime.pid), str(output)]
        if family is not None:
            args.append(family)
        subprocess.run(args, cwd=ROOT, check=True, capture_output=True, timeout=timeout)
        result = json.loads(output.read_text())
        self.runtime.identity()
        if result.get("truncated") or not result.get("topology_unchanged"):
            raise RuntimeError("UI topology moved or exceeded audit bounds; refusing a click")
        return result

    @staticmethod
    def visible_widget(result, suffix):
        candidates = [o["widget"]["center_uv"] for o in result["objects"]
                      if o["path"].endswith(suffix) and o.get("widget")
                      and o["widget"].get("visible") and o["widget"].get("onscreen")]
        if len(candidates) != 1:
            raise RuntimeError(f"A unique visible native UI target is absent: {suffix}")
        return candidates[0]

    @staticmethod
    def picker_present(state):
        return any(v.get("class") == "UIChooseHeroTrain" for v in state.get("frames", []))

    @classmethod
    def picker_ready(cls, state):
        if not cls.picker_present(state):
            return False
        pickers = [o for o in state.get("objects", []) if o.get("class") == "UIChooseHeroTrain"]
        wrappers = [o for o in state.get("objects", []) if o.get("class") == "UIListWrapper"
                    and "ChooseHeroTrain" in o.get("path", "")]
        if len(pickers) != 1 or len(wrappers) != 1:
            return False
        try:
            cls.visible_widget(state, "m_Label_TrainConfirm")
        except RuntimeError:
            return False
        return True

    @classmethod
    def change_target(cls, state):
        if cls.picker_present(state):
            raise RuntimeError("Hero picker is already present; refusing another GM click")
        suffix = "UITrainGM.uiFrame.m_Sprite_F8_Addbgyx"
        point = cls.visible_widget(state, suffix)
        target = next(o for o in state["objects"] if o["path"].endswith(suffix)
                      and o.get("widget") and o["widget"].get("visible") and o["widget"].get("onscreen"))
        owners = [f.get("object") for f in state.get("frames", []) if f.get("class") == "UITrainGM"]
        if len(owners) != 1 or type(owners[0]) is not int or owners[0] <= 4096 or type(target.get("object")) is not int or target["object"] <= 4096:
            raise RuntimeError("Current training GM/button owner is unknown")
        return point, (owners[0], target["object"])

    def open(self):
        # Every observation is retained; a tap ACK alone never means the picker
        # opened. The only retry must use a newly observed, same-owner GM button.
        record = {"format": "clbb-training-picker-open", "pid": self.runtime.pid,
                  "birth": str(self.runtime.birth), "attempts": [], "observations": []}
        path = self.directory / f"{self.serial + 1:03d}-open-observations.json"

        def tap(point, kind):
            self.runtime.identity()
            self.runtime.idle()
            contacts = json.loads(self.runtime.rpc("read diagnostics.contacts"))
            if contacts.get("active") != []:
                raise RuntimeError("Mapped contacts are held or unknown; refusing GM input")
            self.runtime.identity()
            row = {"kind": kind, "point": list(point), "start_ns": time.monotonic_ns(), "raw_idle": True}
            record["observations"].append(row)
            self.runtime.tap(point)
            row.update(completed_ns=time.monotonic_ns(), tap_returned=True)

        def wait_picker(attempt):
            deadline = time.monotonic() + 2.
            while time.monotonic() < deadline:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    break
                row = {"attempt": attempt, "start_ns": time.monotonic_ns()}
                record["observations"].append(row)
                try:
                    state = self.audit(f"picker-open-{attempt}", "ChooseHeroTrain", timeout=remaining)
                except subprocess.TimeoutExpired:
                    row.update(audit_timeout=True, end_ns=time.monotonic_ns())
                    return False
                self.runtime.identity()
                row.update(picker_present=self.picker_present(state), picker_ready=self.picker_ready(state), end_ns=time.monotonic_ns())
                if row["picker_ready"]:
                    return True
                remaining = deadline - time.monotonic()
                if remaining > 0:
                    time.sleep(min(.1, remaining))
            return False

        with path.open("x") as output:
            try:
                state = self.audit("training-menu", "Train")
                if self.picker_ready(state):
                    record["picker_ready"] = True
                    return
                if self.picker_present(state):
                    if wait_picker(0):
                        record["picker_ready"] = True
                        return
                    raise RuntimeError("Existing hero picker did not become ready within2s; no GM retry")
                try:
                    self.visible_widget(state, "UITrainGM.uiFrame.m_Sprite_F8_Addbgyx")
                except RuntimeError:
                    tap(self.visible_widget(state, "UITrainGM.uiFrame.m_Sprite_GmIcon"), "open_gm")
                    state = self.audit("training-menu-open", "Train")
                point, owner = self.change_target(state)
                for attempt in (1, 2):
                    if attempt == 2:
                        self.runtime.identity()
                        self.runtime.idle()
                        state = self.audit("training-menu-retry", "Train")
                        if self.picker_ready(state):
                            record["picker_ready"] = True
                            return
                        point, fresh_owner = self.change_target(state)
                        if fresh_owner != owner:
                            raise RuntimeError("Training GM/button owner changed; refusing retry")
                    record["attempts"].append({"attempt": attempt, "point": list(point), "owner": list(owner)})
                    tap(point, f"change_hero_{attempt}")
                    if wait_picker(attempt):
                        record["picker_ready"] = True
                        return
                raise RuntimeError("Training hero picker did not open after2 bounded attempts; no further input")
            except BaseException as error:
                record["error"] = f"{type(error).__name__}: {error}"
                raise
            finally:
                record["end_ns"] = time.monotonic_ns()
                json.dump(record, output, ensure_ascii=False, indent=2)
                output.write("\n")

    def snapshot(self):
        ui = self.audit("picker", "ChooseHeroTrain")
        picker = next((o for o in ui["objects"] if o["class"] == "UIChooseHeroTrain"), None)
        wrapper = next((o for o in ui["objects"] if o["class"] == "UIListWrapper" and "ChooseHeroTrain" in o["path"]), None)
        if picker is None or wrapper is None:
            raise RuntimeError("Training hero picker is not currently open")
        reader = self.runtime
        obj, wrap = picker["object"], wrapper["object"]
        if reader.class_name(reader.u64(obj)) != "UIChooseHeroTrain":
            raise RuntimeError("Hero picker owner changed")
        order = reader.int_list(reader.u64(reader.field(obj, "m_ShowHeroList")))
        # _curShowStartIndex remains zero during recycling; showStartIndex is the
        # actual data index represented by currentShowList[0]. Observed at bottom
        # showStartIndex96 + slot30 -> native selected hero126, not order[30].
        start = reader.i32(reader.field(wrap, "showStartIndex"))
        columns = reader.i32(reader.field(wrap, "columeCount"))
        if not 0 <= start < len(order) or not 1 <= columns <= 16:
            raise RuntimeError("Invalid native grid position")
        cells = []
        for cell in ui["objects"]:
            if ".currentShowList[" not in cell["path"] or cell["class"] != "GameObject":
                continue
            slot = int(cell["path"].split(".currentShowList[")[1].split("]")[0])
            index = start + slot
            if index < len(order):
                cells.append({"slot": slot, "data_index": index, "hero_id": order[index],
                              "anchor_uv": cell.get("anchor_uv"), "object": cell["object"],
                              "button_geometry": (cell.get("button_widget") or {}).get("geometry")})
        if reader.i32(reader.field(wrap, "showStartIndex")) != start:
            raise RuntimeError("Hero grid scrolled during snapshot")
        result = {"pid": reader.pid, "birth": reader.birth, "read_ns": time.monotonic_ns(),
                  "picker_object": obj, "wrapper_object": wrap, "selected_hero_id": reader.i32(reader.field(obj, "_selectHeroId")),
                  "order": order, "columns": columns, "show_start_index": start, "cells": cells,
                  "confirm_uv": self.visible_widget(ui, "m_Label_TrainConfirm")}
        path = self.directory / f"{self.serial:03d}-picker-state.json"
        path.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
        return result

    def choose(self, hero, max_scrolls):
        last_shape = None
        unchanged = 0
        scrolls = 0
        geometry_retries = 0
        while scrolls <= max_scrolls:
            state = self.snapshot()
            if hero not in state["order"]:
                raise RuntimeError(f"Hero ID{hero} is not in this training picker")
            target_index = state["order"].index(hero)
            candidates = []
            points = []
            target = next((c for c in state["cells"] if c["data_index"] == target_index), None)
            for cell in state["cells"]:
                point, visible = observed_cell_geometry(cell)
                if point is not None:
                    points.append(point)
                press = visible_cell_press(cell)
                if cell["hero_id"] == hero and press is not None:
                    candidates.append(press)
            if len(candidates) > 1:
                raise RuntimeError("Multiple rendered native cells claim the requested hero; refusing a click")
            if len(candidates) == 1:
                self.runtime.tap(candidates[0])
                deadline = time.monotonic() + 2
                while time.monotonic() < deadline:
                    selected = self.runtime.i32(self.runtime.field(state["picker_object"], "_selectHeroId"))
                    if selected == hero:
                        return state
                    time.sleep(0.05)
                raise RuntimeError(f"Clicked cell did not select requested hero ID{hero}; refusing confirmation")
            target_point, _ = observed_cell_geometry(target) if target is not None else (None, False)
            # Pooled cells can exist before their current rendered geometry is
            # available. Reobserve without scrolling/clicking; a cold HUD camera
            # can also leave every GameObject anchor absent even while each
            # UIButton.mWidget has a valid independent camera and click center.
            unknown_target = target is not None and (target_point is None or
                (0.21 <= target_point[1] <= 0.74))
            if not points or unknown_target:
                geometry_retries += 1
                if geometry_retries > 3:
                    raise RuntimeError("Native hero cell geometry/visibility remains unavailable after three reobservations; no blind click or scroll")
                time.sleep(.20)
                continue
            geometry_retries = 0
            shape = state["show_start_index"], tuple(round(point[1], 3) for point in points)
            unchanged = unchanged + 1 if shape == last_shape else 0
            if unchanged >= 2:
                raise RuntimeError("Native grid stopped moving before target became clickable")
            last_shape = shape
            if scrolls == max_scrolls:
                break
            downward = target_index < state["show_start_index"] or (target_point is not None and target_point[1] < 0.21)
            self.runtime.swipe(downward)
            scrolls += 1
        raise RuntimeError("Bounded hero scrolling did not reach the target")

    def confirm(self, hero):
        for stage in range(2):
            ui = self.audit(f"confirm-{stage}", "ChooseHeroTrain")
            picker = next((o for o in ui["objects"] if o["class"] == "UIChooseHeroTrain"), None)
            if picker is None:
                if int(self.runtime.rpc("read hero.id") or "0") == hero:
                    break
                raise RuntimeError("Confirmation dialog changed; refusing an unobserved target")
            if self.runtime.i32(self.runtime.field(picker["object"], "_selectHeroId")) != hero:
                raise RuntimeError("Selected hero ID changed before confirmation")
            self.runtime.tap(self.visible_widget(ui, "m_Label_TrainConfirm"))
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            if int(self.runtime.rpc("read hero.id") or "0") == hero:
                time.sleep(5)
                if int(self.runtime.rpc("read hero.id") or "0") != hero:
                    raise RuntimeError("Hero identity changed during training setup")
                return
            time.sleep(0.10)
        raise RuntimeError("Confirmed hero did not become the local actor")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("hero_id", type=int, nargs="?")
    parser.add_argument("--open-picker", action="store_true", help="Open only the owned training wrench and hero picker")
    parser.add_argument("--inspect-only", action="store_true", help="Read picker state; send no input")
    parser.add_argument("--max-scrolls", type=int, default=12)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.inspect_only and args.open_picker:
        parser.error("--inspect-only cannot open the picker")
    if not args.inspect_only and (args.hero_id is None or not 0 < args.hero_id < 10000):
        parser.error("Choosing requires one valid native hero ID")
    if not 0 <= args.max_scrolls <= 20:
        parser.error("max-scrolls must be0..20")
    runtime = OwnedRuntime()
    directory = args.output or ROOT / "logs/hero-bindings" / f"select-{args.hero_id or 'inspect'}-{time.monotonic_ns()}"
    selector = Selector(runtime, directory)
    if args.inspect_only:
        print(json.dumps(selector.snapshot(), ensure_ascii=False))
        return
    contacts = json.loads(runtime.rpc("read diagnostics.contacts"))
    if contacts.get("active"):
        raise RuntimeError("Mapped contacts are held; release them before selecting a hero")
    if args.open_picker:
        selector.open()
    selector.choose(args.hero_id, args.max_scrolls)
    selector.confirm(args.hero_id)
    result = {"hero": args.hero_id, "pid": runtime.pid, "selected": True, "evidence": str(directory)}
    (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"choose-training-hero: {error}", file=sys.stderr)
        sys.exit(1)
