#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build a clean Linux runtime and matching source companion, never game data."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile

ROOT=Path(__file__).resolve().parents[1]

def run(*args,cwd=ROOT):subprocess.run(args,cwd=cwd,check=True)
def copy(source,target):
    if source.is_symlink():
        link=os.readlink(source)
        if os.path.isabs(link):
            try: relative=Path(link).relative_to(ROOT/'prefix')
            except ValueError: raise ValueError('External symlink cannot be shipped: '+str(source))
            release_prefix=next(p for p in target.parents if p.name=='prefix')
            link=os.path.relpath(release_prefix/relative,target.parent)
        target.parent.mkdir(parents=True,exist_ok=True);target.symlink_to(link)
    elif source.is_dir():
        target.mkdir(parents=True,exist_ok=True)
        for child in sorted(source.iterdir()):copy(child,target/child.name)
    else:
        target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target)

def file_manifest(root):
    files={}
    for p in sorted(root.rglob('*')):
        name=str(p.relative_to(root))
        if p.is_symlink():files[name]={'type':'symlink','target':os.readlink(p)}
        elif p.is_file():files[name]={'type':'file','sha256':hashlib.file_digest(p.open('rb'),'sha256').hexdigest()}
    return files

def tar_tree(tree,path):
    with tarfile.open(path,'w:gz',compresslevel=3)as tar:
        for child in sorted(tree.iterdir()):tar.add(child,arcname=child.name,recursive=True)

def source_archive(destination,tag):
    with tempfile.TemporaryDirectory(prefix='clbb-corresponding-source-')as temp:
        source=Path(temp)/'source';source.mkdir()
        archive=Path(temp)/'project.tar'
        run('git','archive','--format=tar','--output',str(archive),'HEAD')
        with tarfile.open(archive)as tar:tar.extractall(source,filter='data')
        pins=json.loads((ROOT/'sources.json').read_text())
        locations={'qemu':'src/nb-qemu/qemu','nb-qemu-bionic':'src/nb-qemu/libnb-qemu-guest/bionic'}
        for name,pin in pins.items():
            relative=locations.get(name,'src/'+name);directory=source/relative;directory.mkdir(parents=True,exist_ok=True)
            run('git','-C',str(ROOT/relative),'archive','--format=tar','--output',str(archive),pin['commit'])
            # Upstream Android trees contain legitimate links to sibling source
            # directories. Permit those only within the complete source bundle.
            def source_filter(member,destination):
                checked=tarfile.tar_filter(member,destination)
                if checked.issym() or checked.islnk():
                    parent=Path(destination)/checked.name
                    target=(parent.parent/checked.linkname if checked.issym() else Path(destination)/checked.linkname).resolve()
                    if not target.is_relative_to(source.resolve()):raise ValueError('Source link escapes bundle: '+checked.name)
                if not(checked.isfile()or checked.isdir()or checked.issym()or checked.islnk()):raise ValueError('Unsupported source entry')
                return checked
            with tarfile.open(archive)as tar:tar.extractall(directory,filter=source_filter)
        import importlib.util
        spec=importlib.util.spec_from_file_location('exporter',ROOT/'tools/export-runtime-patches.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        for _name,relative,patch,_approved in module.SPECS:
            run('git','apply',str(ROOT/'patches'/patch),cwd=source/relative)
        (source/'SOURCE_RELEASE').write_text(tag+'\n')
        tar_tree(source,destination)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-only',action='store_true');parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--main-executable',type=Path,help='Fresh ATL executable to stage without replacing an active developer runtime')
    parser.add_argument('--main-library',type=Path,help='Fresh ATL main library to stage without installing over a running developer copy')
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    tag=(ROOT/'VERSION').read_text().strip()
    if not re.fullmatch(r'v\d+\.\d+\.\d+(?:-alpha\.\d+)?',tag):raise ValueError('Invalid release version')
    source= args.output/f'clbb-{tag}-sources.tar.gz'
    if args.source_only:source_archive(source,tag);return
    for manifest in ['distribution/manager','launcher','mapper/runtime']:
        command=['cargo','build','--manifest-path',str(ROOT/manifest/'Cargo.toml'),'--release','--offline','--locked']
        if manifest=='mapper/runtime':command+=['--target','x86_64-unknown-linux-gnu']
        run(*command)
    with tempfile.TemporaryDirectory(prefix='clbb-runtime-package-')as temp:
        stage=Path(temp)/'runtime';stage.mkdir()
        for name in ['VERSION','LICENSE','LICENSE_POLICY.md','LICENSES','THIRD_PARTY_NOTICES.txt','legal','README.md','README.en.md','env.sh','run-mlbb.sh','prepare-runtime.py','prepare-native-plugins.py','prepare-system-fonts.py']:
            copy(ROOT/name,stage/name)
        for name in ['import_game.py','verify.py','game-apk.json','launch-fallback.py']:copy(ROOT/'distribution'/name,stage/'distribution'/name)
        for name,source_binary in [('clbb-launcher',ROOT/'launcher/target/release/clbb-launcher'),('clbb-manager',ROOT/'distribution/manager/target/release/clbb-manager')]:copy(source_binary,stage/'bin'/name)
        prefix=ROOT/'prefix'
        for name in ['android-translation-layer','dalvikvm','dex2oat']:copy(prefix/'bin'/name,stage/'prefix/bin'/name)
        for name in ['art','java/dex','atl-web-extensions','libnb-qemu-thunks']:copy(prefix/'lib'/name,stage/'prefix/lib'/name)
        patterns=['libGLESv1_CM.so*','libOpenSLES.so*','libandroid.so*','libc_bio.so*','libdl_bio.so*','libpthread_bio.so*','libstdc++_bio.so*','libnb-qemu.so*','libwolfssl.so*','libclbb_*.so']
        for pattern in patterns:
            for p in sorted((prefix/'lib').glob(pattern)):copy(p,stage/'prefix/lib'/p.name)
        if args.main_library:copy(args.main_library,stage/'prefix/lib/java/dex/android_translation_layer/natives/libtranslation_layer_main.so')
        if args.main_executable:copy(args.main_executable,stage/'prefix/bin/android-translation-layer')
        guest=prefix/'share/libnb-qemu-guest';target=stage/'prefix/share/libnb-qemu-guest';target.mkdir(parents=True)
        for child in sorted(guest.iterdir()):
            if child.name not in {'system','etc','vendor'}:copy(child,target/child.name)
        mapper=ROOT/'mapper/runtime/target/x86_64-unknown-linux-gnu/release/libclbb_mapper.so'
        shutil.copy2(mapper,stage/'prefix/lib/libclbb_mapper.so')
        maximum=(2,17)
        for p in sorted(stage.rglob('*')):
            if p.is_symlink()or not p.is_file():continue
            with p.open('rb')as f:header=f.read(20)
            if len(header)<20 or header[:6]!=b'\x7fELF\x02\x01' or int.from_bytes(header[18:20],'little')!=62:continue
            # Relocate host ELF dependencies; no developer-machine RPATH leaks.
            rpath='$ORIGIN:$ORIGIN/..:$ORIGIN/../lib:$ORIGIN/../lib/art:$ORIGIN/../lib/java/dex/android_translation_layer/natives'
            if p.parent.name=='art':rpath='$ORIGIN:$ORIGIN/..:$ORIGIN/../java/dex/art/natives'
            run('patchelf','--set-rpath',rpath,str(p))
            run('strip','--strip-debug',str(p))
            versions=subprocess.check_output(['readelf','--version-info','--wide',str(p)],text=True)
            if 'Version needs section'in versions:
                needs=versions.split('Version needs section',1)[1]
                for major,minor in re.findall(r'Name: GLIBC_(\d+)\.(\d+)\b',needs):maximum=max(maximum,(int(major),int(minor)))
        dependencies=[line.strip()for line in (ROOT/'distribution/arch-dependencies.txt').read_text().splitlines()if line.strip()and not line.startswith('#')]
        info={'schema':1,'version':tag,'platform':'linux-x86_64','min_glibc':'.'.join(map(str,maximum)),
              'arch_dependencies':dependencies,'game_included':False,'source_asset':source.name,
              'files':file_manifest(stage)}
        (stage/'release.json').write_text(json.dumps(info,indent=2)+'\n')
        run('python3',str(ROOT/'distribution/verify.py'),str(stage),str(stage/'release.json'))
        runtime=args.output/f'clbb-{tag}-linux-x86_64.tar.gz';tar_tree(stage,runtime)
        copy(ROOT/'distribution/manager/target/release/clbb-manager',args.output/'clbb-manager-linux-x86_64')
        # Caller produces the source companion after committing the exact source.
        print(json.dumps({'runtime':str(runtime),'minimum_glibc':info['min_glibc'],'files':len(info['files']),'source_companion_required':str(source)}))
if __name__=='__main__':main()
