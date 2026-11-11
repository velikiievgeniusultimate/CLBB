#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fetch pinned public source revisions and apply only reviewed project patches."""
import importlib.util,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
locations={'qemu':'src/nb-qemu/qemu','nb-qemu-bionic':'src/nb-qemu/libnb-qemu-guest/bionic'}
for name,pin in json.loads((ROOT/'sources.json').read_text()).items():
    path=ROOT/locations.get(name,'src/'+name)
    # Source companion archives already contain full, patched source trees.
    if (ROOT/'SOURCE_RELEASE').is_file():break
    fresh=not (path/'.git').exists()
    if fresh:
        path.parent.mkdir(parents=True,exist_ok=True)
        subprocess.run(['git','clone','--no-checkout',pin['remote'],str(path)],check=True)
    actual=subprocess.check_output(['git','-C',str(path),'rev-parse','HEAD'],text=True).strip()
    if fresh:
        subprocess.run(['git','-C',str(path),'checkout','--detach',pin['commit']],check=True)
    elif actual!=pin['commit']:
        status=subprocess.check_output(['git','-C',str(path),'status','--porcelain'],text=True)
        if status:raise SystemExit('Refusing to discard source changes: '+name)
        subprocess.run(['git','-C',str(path),'checkout','--detach',pin['commit']],check=True)
spec=importlib.util.spec_from_file_location('exporter',ROOT/'tools/export-runtime-patches.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
if not (ROOT/'SOURCE_RELEASE').is_file():
    for _name,relative,patch,_approved in module.SPECS:
        directory=ROOT/relative;file=ROOT/'patches'/patch
        applied=subprocess.run(['git','apply','--reverse','--check',str(file)],cwd=directory,capture_output=True)
        if applied.returncode:
            subprocess.run(['git','apply','--check',str(file)],cwd=directory,check=True)
            subprocess.run(['git','apply',str(file)],cwd=directory,check=True)
print('CLBB pinned sources ready / Исходники CLBB подготовлены')
