# SPDX-License-Identifier: GPL-3.0-or-later
# Owned UI description reader. Caller must visibly open full descriptions first.
import argparse,importlib.util,json,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
s=importlib.util.spec_from_file_location('owned',ROOT/'tools/choose-training-hero.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
a=argparse.ArgumentParser();a.add_argument('--hero',type=int,required=True);a.add_argument('--frozen-manifest',type=Path,required=True);a.add_argument('--output',type=Path,required=True);args=a.parse_args();args.output.mkdir(exist_ok=False);r=m.OwnedRuntime();assert int(r.rpc('read hero.id'))==args.hero
assert args.hero not in {h['id']for h in json.loads(args.frozen_manifest.read_text())['heroes']}
def audit(name,filter):
 p=args.output/(name+'.json');subprocess.run([str(ROOT/'mapper/runtime/target/debug/training-ui-audit'),str(r.pid),str(p),filter],check=True,stdout=subprocess.DEVNULL,cwd=ROOT);d=json.loads(p.read_text());assert d['topology_unchanged'] and not d['truncated'];r.identity();assert int(r.rpc('read hero.id'))==args.hero;return d
initial=audit('panel-before','playerinfo');by={o['path']:o for o in initial['objects']};icons=[]
for i in range(1,5):
 path=f'UIPlayerInfo.m_UIFrameInfo.m_sprite_SkillIcon{i:02}';o=by.get(path);g=o.get('widget')if o else None
 if g and g['visible']and g['onscreen'] and all(0<x<1 for x in g['center_uv']):icons.append((path,g['center_uv']))
assert len(icons)==4,'Only observed three-skill+passive panel supported by this helper'
result={'hero':args.hero,'pid':r.pid,'birth':r.birth,'scope':'Actual localized visible in-game full-description UILabel text, not CData localization keys or guessed names; no skill classification authorization','descriptions':[]}
for i,(path,point)in enumerate(icons):
 r.tap(point);d=audit(f'selected-{i}','playerinfo');rows={o['path']:o for o in d['objects']};texts={}
 for field in ['m_Label_SkillInfoNameFull','m_Label_SkillInfoTipFull_Imageable','m_Label_SkillInfoCDFull','m_Label_SkillInfoMCFull','m_Label_HeroAttrName']:
  o=rows.get('UIPlayerInfo.m_UIFrameInfo.'+field,{})
  if isinstance(o.get('values',{}).get('mText'),str) and ((o.get('widget')or{}).get('visible') or field=='m_Label_SkillInfoTipFull_Imageable'):texts[field]=o['values']['mText']
 assert texts.get('m_Label_SkillInfoNameFull')and texts.get('m_Label_SkillInfoTipFull_Imageable'),'Full localized description absent'
 result['descriptions'].append({'icon':path,'uv':point,'texts':texts,'audit':str(args.output/f'selected-{i}.json'),'body_widget_reported_visibility':(rows['UIPlayerInfo.m_UIFrameInfo.m_Label_SkillInfoTipFull_Imageable'].get('widget')or{}).get('visible'),'visibility_note':'Imageable label visibility can differ from rendered text; compare retained screenshot.'})
 subprocess.run(['python3',str(ROOT/'tools/capture-qemu-window.py'),'--display',':1','--output',str(args.output/f'selected-{i}.png')],check=True,stdout=subprocess.DEVNULL,cwd=ROOT)
# This is the observed native wrench, outside the description panel. In
# this game it dismisses UIPlayerInfo first; verify both overlays afterwards.
train=audit('before-panel-close','traingm')
wrenches=[o['widget']for o in train['objects']if o['path']=='UITrainGM.uiFrame.m_Sprite_GmIcon' and o.get('widget')and o['widget']['visible']and o['widget']['onscreen']]
assert len(wrenches)==1
r.tap(wrenches[0]['center_uv'])
closed=audit('after-panel-close','playerinfo')
assert not any((o.get('widget')or{}).get('visible')for o in closed['objects']if o['path']=='UIPlayerInfo.m_UIFrameInfo.m_Label_HeroAttrName'),'Description panel remains open'
train=audit('after-panel-close-gm','traingm')
assert not any((o.get('widget')or{}).get('visible')for o in train['objects']if o['path']=='UITrainGM.uiFrame.m_Sprite_F2add_MyHero'),'GM overlay remains open'
subprocess.run(['python3',str(ROOT/'tools/capture-qemu-window.py'),'--display',':1','--output',str(args.output/'panel-closed.png')],check=True,stdout=subprocess.DEVNULL,cwd=ROOT)
result['description_panel_closed']=True
(args.output/'descriptions.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2))
