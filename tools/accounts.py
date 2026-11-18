#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Local account window coordinator. Authentication stays in Android profiles."""
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import socket
import socketserver
import struct
import subprocess
import sys
import threading
import time
import uuid
from typing import NamedTuple

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("instances", ROOT / "tools/debug-instance.py")
instances = importlib.util.module_from_spec(spec)
spec.loader.exec_module(instances)
BASE = instances.BASE
ADDRESS = BASE / ".accounts.sock"
REGISTRY = BASE / ".accounts.json"
PRIMARY = "primary"
WRAPPER = ROOT / ".clbb-local-launch"
LOCK = threading.Lock()
BOOT_ID = Path("/proc/sys/kernel/random/boot_id").read_text().strip()


class Launch(NamedTuple):
    child: subprocess.Popen
    started_ns: int


STARTING = {}


def registry():
    default = {"selected": PRIMARY, "labels": {}}
    if REGISTRY.exists():
        default.update(json.loads(REGISTRY.read_text()))
    return default


def names():
    labels = registry()["labels"]
    result = {PRIMARY: labels.get(PRIMARY, "Основной аккаунт")}
    if not BASE.exists():
        return result
    for folder in sorted(BASE.iterdir()):
        marker = folder / ".clbb-debug-instance.json"
        try:
            if folder.is_symlink() or not marker.is_file():
                continue
            meta = json.loads(marker.read_text())
            if meta.get("source_root") == str(ROOT) and meta.get("state_root") == str(folder):
                result[folder.name] = labels.get(folder.name, "Дополнительный аккаунт" if folder.name == "secondary" else folder.name)
        except (OSError, ValueError):
            continue
    return result


def owned(account):
    try:
        if account != PRIMARY:
            return instances.probe(instances.profile(account))
        spec = importlib.util.spec_from_file_location("probe", ROOT / "tools/probe-hero-control.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module.Probe(0, {"commands": [], "samples": []})
    except (OSError, RuntimeError, ValueError):
        return None


def running(account):
    launch = STARTING.get(account)
    if launch is not None:
        if launch.child.poll() is None:
            return True
        STARTING.pop(account)
    return bool(owned(account))


def start(account):
    if running(account):
        return
    if sum(running(name) for name in names()) >= 2:
        raise ValueError("Уже открыты два окна CLBB.")
    started_ns = time.monotonic_ns()
    if account == PRIMARY:
        env = dict(os.environ, CLBB_ACCOUNT_BYPASS="1")
        with (ROOT / "logs/account-launch.log").open("ab") as log:
            child = subprocess.Popen([str(WRAPPER)], env=env, stdin=subprocess.DEVNULL,
                                     stdout=log, stderr=log, start_new_session=True)
    else:
        child = instances.start_supervisor(account)
    STARTING[account] = Launch(child, started_ns)


def desktop_close_requested(account, launch):
    logs = (ROOT if account == PRIMARY else instances.profile(account)) / "logs"
    try:
        current = json.loads((logs / "current-atl-pid.json").read_text())
        start, pid = current["start_monotonic_ns"], current["launcher_pid"]
        if current["boot_id"] != BOOT_ID or start < launch.started_ns or pid <= 0:
            return False
        # The window close handler writes this line immediately before _exit.
        # The previous run's marker must not turn a second startup click into a relaunch.
        with (logs / f"qemu-launch-{start}-{pid}" / "runtime.log").open("rb") as log:
            log.seek(max(0, log.seek(0, os.SEEK_END) - 4096))
            marker = b"[CLBB_EXIT] desktop_close"
            return any(line.lstrip(b"\0") == marker for line in log.read().splitlines())
    except (OSError, ValueError, TypeError, KeyError):
        return False


def launch_from_desktop():
    with LOCK:
        account = registry()["selected"]
        previous = STARTING.get(account)
        if (previous is None or previous.child.poll() is not None
                or not desktop_close_requested(account, previous)):
            start(account)
            return

    # The window is closing, but its supervisor still owns the launch. Wait
    # outside the coordinator lock so the other profile stays usable.
    try:
        previous.child.wait(timeout=25)
    except subprocess.TimeoutExpired:
        raise ValueError("Предыдущее окно CLBB ещё завершает работу.") from None
    with LOCK:
        if registry()["selected"] == account:
            start(account)


def close(account):
    process = owned(account)
    if not process:
        return
    try:
        process.command("close")
    except (OSError, RuntimeError):
        # The window can close before the socket sends its acknowledgement.
        pass
    until = time.monotonic() + 6
    while owned(account) and time.monotonic() < until:
        time.sleep(.05)
    if owned(account):
        raise ValueError("Текущее окно не закрылось.")
    launch = STARTING.get(account)
    if launch is not None:
        try:
            launch.child.wait(timeout=max(0, until - time.monotonic()))
        except subprocess.TimeoutExpired:
            raise ValueError("Текущее окно ещё закрывается.") from None
        STARTING.pop(account)


def profile_label(text):
    label = text.strip()
    if not label or len(label) > 48 or any(not character.isprintable() for character in label):
        raise ValueError("Введите название длиной до 48 символов.")
    return label


def dispatch(account, action, target):
    if action == "list":
        return "\n".join(f"{name}\t{label}\t{int(running(name))}\t{int(name == account)}"
                         for name, label in names().items())
    if action == "add":
        label = profile_label(target)
        if sum(running(name) for name in names()) >= 2:
            raise ValueError("Закройте одно окно перед добавлением аккаунта.")
        target = "account-" + uuid.uuid4().hex[:16]
        instances.prepare(target, interactive=True)
        data = registry()
        data["labels"][target] = label
        instances.write(REGISTRY, data)
        start(target)
        return "Профиль добавлен. Войдите в аккаунт в новом окне игры."
    if action == "rename":
        profile, separator, new_label = target.partition("\t")
        if not separator or profile not in names():
            raise ValueError("Выберите существующий профиль.")
        label = profile_label(new_label)
        data = registry()
        data["labels"][profile] = label
        instances.write(REGISTRY, data)
        return "Профиль переименован."
    if target not in names() or target == account:
        raise ValueError("Выберите другой аккаунт.")
    if action == "second":
        if running(target):
            raise ValueError("Этот аккаунт уже открыт в другом окне.")
        start(target)
        return "Открываю второе окно."
    if action == "switch":
        data = registry()
        close(account)
        try:
            start(target)
        except Exception:
            # Recover the source window when the destination could not start.
            start(account)
            raise
        data["selected"] = target
        instances.write(REGISTRY, data)
        return "Аккаунт переключён."
    raise ValueError("Неизвестное действие.")


def peer_account(pid):
    for name in names():
        process = owned(name)
        if process and process.pid == pid:
            process.identity()
            return name
    return None


class Handler(socketserver.StreamRequestHandler):
    def handle(self):
        self.connection.settimeout(2)
        pid, uid, _ = struct.unpack("3i", self.connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        if uid != os.getuid():
            return
        request = self.rfile.readline(1024)
        if not request.endswith(b"\n"):
            return
        try:
            verb, _, target = request.decode().rstrip("\n").partition("\t")
            if verb == "desktop" and target == "":
                # The host shortcut can only launch the already selected profile.
                launch_from_desktop()
                reply = ""
            else:
                with LOCK:
                    account = peer_account(pid)
                    if account is None:
                        raise ValueError("Окно CLBB не распознано.")
                    reply = dispatch(account, verb, target)
            self.wfile.write(("OK\n" + reply).encode())
        except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
            try:
                self.wfile.write(("ERR\n" + str(error)).encode())
            except OSError:
                pass


def serve():
    BASE.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.umask(0o077)
    with (BASE / ".accounts.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        ADDRESS.unlink(missing_ok=True)
        with socketserver.ThreadingUnixStreamServer(str(ADDRESS), Handler) as server:
            server.daemon_threads = True
            try:
                server.serve_forever()
            finally:
                ADDRESS.unlink(missing_ok=True)


def desktop():
    BASE.mkdir(parents=True, exist_ok=True, mode=0o700)
    for attempt in range(40):
        try:
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(30)
                client.connect(str(ADDRESS))
                client.sendall(b"desktop\n")
                response = client.makefile().read()
            if not response.startswith("OK\n"):
                raise SystemExit(response.removeprefix("ERR\n").strip() or "Не удалось запустить CLBB.")
            return
        except (ConnectionRefusedError, FileNotFoundError):
            if attempt == 0:
                args = ["systemd-run", "--user", "--quiet", "--collect", "--unit=clbb-accounts"]
                for key in ("DISPLAY", "XAUTHORITY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
                    if key in os.environ:
                        args.append(f"--setenv={key}={os.environ[key]}")
                subprocess.run([*args, sys.executable, str(Path(__file__).resolve()), "serve"], check=False)
            time.sleep(.1)
    raise SystemExit("Служба аккаунтов CLBB не запустилась.")


if __name__ == "__main__":
    if sys.argv[1:] == ["serve"]:
        serve()
    elif sys.argv[1:] == ["desktop"]:
        desktop()
    else:
        raise SystemExit("Usage: accounts.py desktop|serve")
