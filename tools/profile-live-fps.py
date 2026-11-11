#!/usr/bin/env python3
"""Bounded, read-only FPS/CPU/GPU sampling of this checkout's exact owned runtime."""
import argparse,json,os,socket,time,statistics
from pathlib import Path
from runtime_identity import runtime_executable_is_owned
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--seconds',type=float,default=15);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
m=json.loads((ROOT/'logs/current-atl-pid.json').read_text());pid=m['pid'];proc=Path('/proc')/str(pid)
if not runtime_executable_is_owned(pid,ROOT):raise SystemExit('Owned runtime changed')
def read(key):
 with socket.socket(socket.AF_UNIX)as c:
  c.settimeout(1);c.connect(f'/run/user/{os.getuid()}/clbb/mapper-{pid}.sock');c.sendall(('read '+key).encode());c.shutdown(socket.SHUT_WR);return c.recv(32768).decode().strip()
def tasks():
 r={}
 for t in (proc/'task').iterdir():
  try:
   v=(t/'stat').read_text().rsplit(')',1)[1].split();r[t.name]=[int(v[11])+int(v[12]),(t/'comm').read_text().strip()]
  except OSError:pass
 return r
rows=[];start=tasks();begin=time.monotonic()
while time.monotonic()-begin<a.seconds:
 if not runtime_executable_is_owned(pid,ROOT):raise SystemExit('Runtime exited during sample')
 t=time.monotonic();v=json.loads(read('diagnostics.render'));active=min(v['surfaces'],key=lambda s:s['gtk_handoff'].get('last_age_ms')if s['gtk_handoff'].get('last_age_ms')is not None else 1e12)
 gpu={}
 for card in Path('/sys/class/drm').glob('card[0-9]*'):
  try:gpu[card.name]=int((card/'device/gpu_busy_percent').read_text())
  except OSError:pass
 rows.append({'time':t,'surface':active['surface'],'size':active['size'],'render':active['gtk_handoff'],'snapshot':active.get('gtk_snapshot'),'gpu':gpu})
 time.sleep(max(0,.5-(time.monotonic()-t)))
end=tasks();elapsed=time.monotonic()-begin;hz=os.sysconf('SC_CLK_TCK');threads=sorted([{'tid':k,'name':v[1],'core_percent':round((v[0]-start.get(k,v)[0])/hz/elapsed*100,2)}for k,v in end.items()],key=lambda x:-x['core_percent'])
interval_fps=[(b['render']['total']-c['render']['total'])/(b['time']-c['time'])for c,b in zip(rows,rows[1:])if b['surface']==c['surface']and b['render']['total']>=c['render']['total']]
summary={'pid':pid,'seconds':elapsed,'fps_mean':statistics.mean(interval_fps),'fps_median':statistics.median(interval_fps),'fps_min':min(interval_fps),'frame_p95_ms_mean':statistics.mean(r['render']['interval_p95_ms'] for r in rows if r['render']['interval_p95_ms'] is not None),'gpu_mean':{k:statistics.mean(r['gpu'][k]for r in rows if k in r['gpu'])for k in rows[0]['gpu']},'cpu_total_cores':sum(t['core_percent']for t in threads)/100,'threads':threads[:15],'size':rows[-1]['size'],'recording':read('debug.recording'),'fps_hud':read('display.show_fps'),'frame_pacing':read('diagnostics.frame_pacing')}
painted=[(b['snapshot']['total']-c['snapshot']['total'])/(b['time']-c['time'])for c,b in zip(rows,rows[1:])if b.get('snapshot') and c.get('snapshot') and b['surface']==c['surface'] and b['snapshot']['total']>=c['snapshot']['total']]
summary['snapshot_fps_mean']=statistics.mean(painted)if painted else None
summary['source_ms']=read('diagnostics.source_ms')
summary['job_worker_count']=sum(t['name'].startswith('Job.worker')for t in threads)
a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps({'summary':summary,'samples':rows},ensure_ascii=False,indent=2)+'\n');print(json.dumps(summary,ensure_ascii=False))
