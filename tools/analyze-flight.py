#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Summarize capture coverage and input failures without running the game."""
import argparse,collections,json,os
from pathlib import Path

def analyze(path):
    with Path(path).open()as stream:
        header=json.loads(next(stream));rows=[json.loads(line)for line in stream if line.strip()]
    counts=collections.Counter(row['kind']for row in rows)
    rejects=collections.Counter(row['data'].get('reason')for row in rows if row['kind']=='core_decision'and row['data'].get('event')=='direction_feedback_rejected')
    packets={}
    for row in rows:
        if row['kind']=='android_motion':
            item=packets.setdefault(row['data']['packet'],{})
            item.setdefault(row['data']['stage'],[]).append(row)
    missing_after=[];missing_reader=[];max_reader_lag=0
    for packet,stages in packets.items():
        if 'before_dispatch'not in stages:continue
        before=stages['before_dispatch'][0]
        if before['ns']>header['end_ns']-100_000_000:continue
        if 'after_dispatch'not in stages:missing_after.append(packet)
        if 'motion_event_reader'not in stages:missing_reader.append(packet)
        else:max_reader_lag=max(max_reader_lag,max((row['ns']-before['ns'])/1e6 for row in stages['motion_event_reader']))
    source=[row for row in rows if row['kind']=='source']
    gaps=[(b['ns']-a['ns'])/1e6 for a,b in zip(source,source[1:])]
    expected=['native_transport','host_event','runtime_lifecycle','input_event','input_route','touch_command','android_motion','source','typed_frame','core_decision','native_ui_audit','controller','controller_state','skill_result']
    # Missing channels are observations, not automatically bugs: an idle/menu clip
    # legitimately lacks skill results or Android touch commands.
    return {'file':str(path),'schema':header.get('version'),'context':header.get('context'),
        'marks':header.get('marks'),'truncated':header.get('truncated'),
        'event_count':len(rows),'channels':dict(counts),'channels_not_observed':[kind for kind in expected if not counts[kind]],
        'direction_rejections':dict(rejects),'android_packets':len(packets),
        'packets_without_dispatch_return':missing_after,'packets_without_reader_in_window':missing_reader,
        'max_motion_reader_lag_ms':max_reader_lag,'max_source_gap_ms':max(gaps,default=None),
        'transport_rejections':[row['data']for row in rows if row['kind']=='native_transport'and not row['data'].get('accepted')],
        'failed_touch_callbacks':sum(row['kind']=='touch_command'and row['data'].get('phase')=='after_callback'and not row['data'].get('accepted')for row in rows)}
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('file',nargs='?');parser.add_argument('--output',type=Path);args=parser.parse_args()
    directory=Path(os.environ.get('XDG_STATE_HOME',Path.home()/'.local/state'))/'clbb/aim-debug'
    try:path=Path(args.file)if args.file else max(directory.glob('clip-*.jsonl'),key=lambda p:p.stat().st_mtime)
    except ValueError:raise SystemExit('No saved recorder clips / Нет сохранённых фрагментов')
    text=json.dumps(analyze(path),ensure_ascii=False,indent=2)
    if args.output:args.output.write_text(text+'\n')
    print(text)
