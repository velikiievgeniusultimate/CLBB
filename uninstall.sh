#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Full application reset; distribution-managed system packages are preserved.
set -eu
python3 - <<'PY'
from pathlib import Path
import os,fcntl,shutil
if os.getuid()==0:raise SystemExit('Run as the desktop user, without sudo / Запускайте без sudo')
home=Path.home();data=Path(os.environ.get('XDG_DATA_HOME',home/'.local/share'));config=Path(os.environ.get('XDG_CONFIG_HOME',home/'.config'));state=Path(os.environ.get('XDG_STATE_HOME',home/'.local/state'))
root=Path(os.environ.get('CLBB_INSTALL_ROOT',data/'clbb')).absolute()
if root.resolve() in [Path('/'),home.resolve(),data.resolve(),config.resolve(),state.resolve()]:raise SystemExit('Unsafe installation root: refusing reset')
if root.exists()and not(root/'releases').is_dir():raise SystemExit('Not a managed CLBB installation: '+str(root))
locks=[]
for lockpath in [root/'state/logs/clbb-launch.lock',root/'.update.lock']:
 if lockpath.exists():
  lock=lockpath.open('a+b')
  try:fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
  except BlockingIOError:raise SystemExit('Close CLBB and its installer before resetting / Сначала закройте CLBB и установщик')
  locks.append(lock)
paths=[root,config/'clbb',state/'clbb',home/'.cache/clbb',home/'.local/bin/clbb',data/'applications/org.clbb.CLBB.desktop']
removed=0
for path in paths:
 if path.is_symlink()or path.is_file():path.unlink();removed+=1;print('Removed / Удалено:',path)
 elif path.is_dir():
  removed+=1;print('Removing / Удаление:',path,flush=True);shutil.rmtree(path)
print('CLBB reset complete / CLBB полностью сброшен. APK, game account, settings, caches and local reports removed. System packages kept. Paths removed:',removed)
PY
