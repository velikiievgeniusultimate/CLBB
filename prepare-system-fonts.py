#!/usr/bin/env python3
"""Expose installed, real font files to both Android ABI namespaces.

Revalidated on launch, written only when changed; no host fonts are copied into a release or renamed to
pretend that their embedded family is another font.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET

def write_if_changed(path: Path, data: bytes) -> None:
    try:
        if path.read_bytes() == data:
            return
    except FileNotFoundError:
        pass
    # Preserve a stable config identity on the normal unchanged launch path;
    # publish complete replacements when host font discovery actually changes.
    pending = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".clbb-font-", delete=False) as output:
            pending = Path(output.name)
            output.write(data)
        pending.chmod(0o644)
        os.replace(pending, path)
        pending = None
    finally:
        if pending is not None:
            pending.unlink(missing_ok=True)


root = Path(os.environ.get('CLBB_STATE_ROOT', Path(__file__).resolve().parent))
system = root/'system'
fonts = system/'fonts'
etc = system/'etc'
fonts.mkdir(parents=True, exist_ok=True)
etc.mkdir(parents=True, exist_ok=True)
listing = subprocess.check_output(['fc-list','--format','%{file}\t%{family[0]}\t%{style[0]}\t%{index}\n'],text=True)
entries = {}
for row in listing.splitlines():
    fields = row.split('\t')
    if len(fields) != 4: continue
    path, family, style, index = fields
    file = Path(path)
    if not file.is_file() or not file.name.startswith(('Noto','DejaVuSans','DejaVuSerif')): continue
    # A TTC needs one entry per face; use its default face for the fallback.
    if index != '0': continue
    if 'Regular' not in style and file.name not in ('DejaVuSans.ttf','DejaVuSansMono.ttf','DejaVuSerif.ttf','NotoColorEmoji.ttf'): continue
    entries.setdefault(file.name, (file, family))
required = ['NotoSans-Regular.ttf','NotoSerif-Regular.ttf','NotoSansMono-Regular.ttf']
substitutes = ['DejaVuSans.ttf','DejaVuSerif.ttf','DejaVuSansMono.ttf']
selected = []
for preferred, fallback in zip(required, substitutes):
    chosen = preferred if preferred in entries else fallback
    if chosen not in entries: raise SystemExit('Missing system font: '+preferred+' or '+fallback)
    selected.append(chosen)
for name,(file,_) in entries.items():
    target=fonts/name
    if target.is_symlink() and target.resolve()==file.resolve(): continue
    if target.exists() and not target.is_symlink(): raise SystemExit('Refusing to replace font file: '+str(target))
    if target.is_symlink():target.unlink()
    target.symlink_to(file)
familyset=ET.Element('familyset',version='22')
for family,name in zip(['sans-serif','serif','monospace'],selected):
    element=ET.SubElement(familyset,'family',name=family)
    ET.SubElement(element,'font',weight='400',style='normal').text=name
for alias in ['arial','Arial','Arial Unicode MS','helvetica','Roboto','Droid Sans','sans']:
    ET.SubElement(familyset,'alias',name=alias,to='sans-serif')
for name in sorted(set(entries)-set(selected),key=lambda value:(value=='NotoColorEmoji.ttf',value)):
    element=ET.SubElement(familyset,'family')
    ET.SubElement(element,'font',weight='400',style='normal').text=name
ET.indent(familyset)
xml=ET.tostring(familyset,encoding='utf-8',xml_declaration=True)
write_if_changed(etc/'fonts.xml', xml)
# Older native Unity font enumerators still use the legacy schema.
for filename,include_names in [('system_fonts.xml',True),('fallback_fonts.xml',False)]:
    legacy=ET.Element('familyset')
    for position,name in enumerate(selected+sorted(set(entries)-set(selected))):
        element=ET.SubElement(legacy,'family')
        if include_names and position<3:
            names=ET.SubElement(element,'nameset');ET.SubElement(names,'name').text=['sans-serif','serif','monospace'][position]
        files=ET.SubElement(element,'fileset');ET.SubElement(files,'file').text=name
    ET.indent(legacy);write_if_changed(etc/filename, ET.tostring(legacy,encoding='utf-8',xml_declaration=True))
# Unity versions use both Android-root-relative and absolute legacy paths.
for directory in [system.parent/'etc',system.parent/'vendor/etc']:
    directory.mkdir(parents=True,exist_ok=True)
    for name in ['fonts.xml','system_fonts.xml','fallback_fonts.xml']:
        target=directory/name
        if not target.exists() and not target.is_symlink():target.symlink_to(etc/name)
cache=root/'cache';cache.mkdir(exist_ok=True)
write_if_changed(cache/'system-fonts-manifest.json', (json.dumps({'count':len(entries),'primary':selected,'files':{name:str(file) for name,(file,_) in entries.items()}},ensure_ascii=False,indent=2,sort_keys=True)+'\n').encode('utf-8'))
print('CLBB_SYSTEM_FONTS_READY='+str(len(entries)))
