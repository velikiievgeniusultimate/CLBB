# SPDX-License-Identifier: GPL-3.0-or-later
"""Installed account backend for the same coordinator used by the workbench."""
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import struct
import subprocess
import tempfile

INSTALL = Path(os.environ["CLBB_INSTALL_ROOT"]).resolve()
PRIMARY_STATE = INSTALL / "state"
BASE = INSTALL / "profiles"
RUN = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
ADDRESS = RUN / "clbb" / ("accounts-" + hashlib.sha256(os.fsencode(INSTALL)).hexdigest()[:16] + ".sock")


def write(path, value):
    with tempfile.NamedTemporaryFile(dir=path.parent, mode="w", delete=False) as stream:
        json.dump(value, stream, ensure_ascii=False)
    os.replace(stream.name, path)


def profile(name):
    if name == "primary":
        return PRIMARY_STATE
    if not re.fullmatch(r"account-[0-9a-f]{16}", name):
        raise ValueError("Недопустимый профиль.")
    return BASE / name


def prepare(name, *, interactive=True):
    state = profile(name)
    state.mkdir(mode=0o700)
    for directory in ("logs", "config", "userdata", "state", "code_cache", "tmp"):
        (state / directory).mkdir(mode=0o700)
    # APKs are installation media. Each profile gets its own Android and WebKit storage.
    (state / "game").symlink_to(PRIMARY_STATE / "game", target_is_directory=True)
    write(state / ".clbb-debug-instance.json", {"source_root": str(PRIMARY_STATE), "state_root": str(state)})
    return state


class Window:
    def __init__(self, state):
        self.state = state
        record = json.loads((state / "logs/current-atl-pid.json").read_text())
        self.pid = int(record["pid"])
        self.record = record
        self.identity()

    def identity(self):
        proc = Path("/proc") / str(self.pid)
        exe = Path(self.record["runtime_root"]) / "prefix/bin/android-translation-layer"
        start = int((proc / "stat").read_text().rsplit(")", 1)[1].split()[19])
        if (not exe.is_relative_to(INSTALL / "releases") or proc.stat().st_uid != os.getuid()
                or start != self.record["pid_start_ticks"] or (proc / "exe").resolve() != exe
                or self.record["boot_id"] != Path("/proc/sys/kernel/random/boot_id").read_text().strip()):
            raise ValueError("Окно CLBB уже сменилось.")

    def command(self, command):
        self.identity()
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(3)
            client.connect(str(RUN / "clbb" / f"mapper-{self.pid}.sock"))
            pid, uid, _ = struct.unpack("3i", client.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
            if (pid, uid) != (self.pid, os.getuid()):
                raise ValueError("Окно CLBB уже сменилось.")
            client.sendall(command.encode())
            client.shutdown(socket.SHUT_WR)
            client.recv(1024)


def probe(state):
    return Window(state)


def start_supervisor(name):
    state = profile(name)
    runtime = (INSTALL / "current").resolve(strict=True)
    env = dict(os.environ, CLBB_ROOT=str(runtime), CLBB_STATE_ROOT=str(state),
               CLBB_ACCOUNT_CHILD="1", CLBB_ACCOUNTS_SOCKET=str(ADDRESS))
    env.pop("CLBB_WRAPPED", None)
    command = [str(runtime / "bin/clbb-launcher")]
    if name != "primary":
        if not (PRIMARY_STATE / "game/base.apk").is_file():
            raise ValueError("Сначала установите игру в основном профиле.")
        env.update(XDG_CONFIG_HOME=str(state / "config"), XDG_DATA_HOME=str(state / "userdata"),
                   XDG_STATE_HOME=str(state / "state"), QEMU_TEMPDIR=str(state / "code_cache"))
        # Namespace-local /tmp keeps the game's legacy fixed temp paths apart.
        # The X socket remains available to both visible windows.
        command = ["bwrap", "--bind", "/", "/", "--dev-bind", "/dev", "/dev",
                   "--bind", str(state / "tmp"), "/tmp", "--ro-bind", "/tmp/.X11-unix", "/tmp/.X11-unix",
                   "--", *command]
        if os.environ.get("XAUTHORITY", "").startswith("/tmp/"):
            auth = env["XAUTHORITY"]
            command[-2:-2] = ["--ro-bind", auth, auth]
    (state / "logs").mkdir(parents=True, exist_ok=True)
    with (state / "logs/account-launch.log").open("ab") as log:
        return subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL,
                                stdout=log, stderr=log, start_new_session=True)
