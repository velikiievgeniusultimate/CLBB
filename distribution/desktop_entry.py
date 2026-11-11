#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Register the installed launcher and notify desktop application menus."""
import os
from pathlib import Path
import shutil
import subprocess


def register(home=None, data=None):
    home = Path(home or os.environ['HOME'])
    data = Path(data or os.environ.get('XDG_DATA_HOME') or home / '.local/share')
    launcher = home / '.local/bin/clbb'
    if not launcher.is_file() or not os.access(launcher, os.X_OK):
        raise RuntimeError('CLBB launcher missing / Не найден установленный launcher: ' + str(launcher))
    exe = str(launcher).replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%')
    directory = data / 'applications'
    directory.mkdir(parents=True, exist_ok=True)
    desktop = directory / 'org.clbb.CLBB.desktop'
    temporary = desktop.with_suffix('.tmp')
    temporary.write_text('[Desktop Entry]\nType=Application\nName=CLBB\nGenericName=Computer Legends Bang Bang\nComment=Computer Legends Bang Bang\nExec="' + exe + '"\nTerminal=false\nCategories=Game;\nKeywords=Mobile Legends;MLBB;CLBB;\nIcon=applications-games\nStartupNotify=false\n', encoding='utf-8')
    temporary.chmod(0o644)
    temporary.replace(desktop)
    # KDE can retain its application-service cache after a terminal installation.
    # Run in the installing desktop session; never use sudo for a user's menu.
    for tool, args in [('update-desktop-database', [str(directory)]), ('kbuildsycoca6', ['--noincremental']), ('kbuildsycoca5', ['--noincremental'])]:
        executable = shutil.which(tool)
        if not executable:
            continue
        if tool == 'kbuildsycoca5' and shutil.which('kbuildsycoca6'):
            continue
        try:
            result = subprocess.run([executable, *args], capture_output=True, text=True, timeout=30)
            if result.returncode:
                print('CLBB: menu refresh warning / предупреждение обновления меню:', tool, result.stderr.strip())
        except (OSError, subprocess.TimeoutExpired) as error:
            print('CLBB: menu refresh warning:', error)
    print('Ярлык / Application shortcut:', desktop)
    return desktop


if __name__ == '__main__':
    try:
        register()
    except (OSError, RuntimeError) as error:
        raise SystemExit(str(error))
