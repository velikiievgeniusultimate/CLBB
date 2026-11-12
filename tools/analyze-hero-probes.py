#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Read-only post-analysis. Invoke with completed probe .json and optional watch .jsonl.
Never controls the runtime. Counter evidence must be from coherent local Actor receipts.
"""
import argparse,json,math,statistics,pathlib

def summary(values):
    values=sorted(v for v in values if isinstance(v,(int,float)) and math.isfinite(v))
    return None if not values else dict(n=len(values),min=values[0],median=statistics.median(values),p95=values[min(len(values)-1,int(len(values)*.95))],max=values[-1])

def vector(value,n=2):
    return isinstance(value,list) and len(value)>=n and all(isinstance(v,(int,float)) and math.isfinite(v) for v in value[:n])

def release_state(s,movement):
    d=s.get('data',{});i=d.get('input',{});cs=d.get('contacts',{}).get('active'); mid=i.get('movement_touch',-1) if movement else -1
    if not isinstance(cs,list):return None
    return i.get('touch',0)<0 and not i.get('pending_release',1) and all(c==mid for c in cs)

def receipt(obj):
    o=obj.get('Ok') if isinstance(obj,dict) else None
    if not isinstance(o,dict) or not o.get('same_match'):return None
    vals=o.get('values',{})
    if not vals or not all(type(v) is int for v in vals.values()):return None
    return dict(ns=o.get('end_ns'),hero=o.get('hero'),values=vals)

def watch(path):
    with pathlib.Path(path).open() as source:rows=[json.loads(line) for line in source if line.strip()]
    header=next((r for r in rows if r.get('event')=='header'),{});footer=next((r for r in reversed(rows) if r.get('event')=='summary'),{})
    rec=[]
    for row in rows:
        for name in ('initial_cast_receipt','cast_receipt','final_cast_receipt'):
            r=receipt(row.get(name));
            if r and r['hero']==header.get('hero'):rec.append(r)
    rec.sort(key=lambda r:r['ns'])
    source=[r for r in rows if r.get('event')=='source' and r.get('same_match')]
    return dict(path=str(path),hero=header.get('hero'),skill=header.get('skill'),footer=footer,receipts=rec,source=source)

def range_observation(sample,probe):
    inputs=sample.get('data',{}).get('input',{});aim=sample.get('data',{}).get('aim',{})
    phase='settled' if sample.get('stage','').endswith(':settled') else 'immediate'
    goal=sample.get('pointer_goal',{}).get('command_end_ns')
    if not isinstance(goal,(int,float)):
        commands=[c for c in probe.get('commands',[]) if c.get('command','').startswith('pointer ')
                  and isinstance(c.get('end_ns'),int) and c['end_ns']<=sample.get('start_ns',0)]
        goal=max((c['end_ns'] for c in commands),default=None)
    # Both sequence numbers must come from one aim RPC snapshot. The earlier
    # input RPC can describe a command superseded before aim was collected.
    matched=aim.get('matched_seq');sent=aim.get('sent_seq')
    eligible=(not sample.get('truncated_at_hold_deadline') and inputs.get('direct') and inputs.get('accepted')
              and aim.get('logical_fresh') and aim.get('desired_valid')
              and vector(aim.get('desired')) and vector(aim.get('offset')))
    down=probe.get('skill_down_ns')
    post_goal=(type(down) is int and isinstance(goal,(int,float)) and goal>=down and aim.get('logical_start_ns',0)>=goal
               and aim.get('accepted_ns',0)>=goal and type(matched) is int and matched>0)
    exact=post_goal and type(sent) is int and sent>0 and sent==matched
    error=None
    if eligible:
        desired=aim['desired'][:2];radius=aim.get('range',0);length=math.hypot(*desired)
        if radius>0 and length>radius:desired=[x*radius/length for x in desired]
        error=math.dist(desired,aim['offset'][:2])
    classification=('unavailable' if error is None else 'previous_or_unproven_goal' if not post_goal
                    else 'post_goal_temporal_only' if type(sent) is not int else
                    'current_goal_command_ack' if exact else 'newest_command_not_acknowledged')
    return dict(phase=phase,goal_command_end_ns=goal,matched_seq=matched,sent_seq=sent,
                source_after_pointer_goal=bool(post_goal),exact_current_goal_ack=bool(exact and error is not None),
                error_world=error,classification=classification)

def analyze(path,watches):
    p=json.loads(pathlib.Path(path).read_text());samples=p.get('samples',[]);down=p.get('skill_down_ns');up=p.get('physical_up_ns');hero=p.get('request',{}).get('hero');movement=p.get('request',{}).get('movement',False)
    held=[s for s in samples if s.get('stage','').startswith('held:')];late=[s for s in held if down and s.get('end_ns',0)>=down+300_000_000]; seq=[];errors=[];sent=[];dirs=[]
    feedback={phase:dict(samples=0,observed=[],post_goal=[],confirmed=[],classifications={}) for phase in ('immediate','settled')}
    for s in held:
        d=s.get('data',{});i=d.get('input',{});a=d.get('aim',{});slot={81:'skill:0',87:'skill:1',69:'skill:2'}.get(p.get('request',{}).get('key'))
        form=next((b.get('form') for b in d.get('cast_lock',{}).get('buttons',[]) if b.get('slot')==slot),None)
        observed=range_observation(s,p);bucket=feedback[observed['phase']];bucket['samples']+=1
        name=observed['classification'];bucket['classifications'][name]=bucket['classifications'].get(name,0)+1
        if observed['error_world'] is not None:
            bucket['observed'].append(observed['error_world'])
            if observed['source_after_pointer_goal']:bucket['post_goal'].append(observed['error_world'])
            if observed['exact_current_goal_ack']:bucket['confirmed'].append(observed['error_world']);errors.append(observed['error_world'])
        seq.append(dict(stage=s['stage'],ms=round((s['end_ns']-down)/1e6,3) if down else None,form=form,core=i.get('skill'),touch=i.get('touch'),direction_fresh=i.get('direction_fresh'),direction_ack=i.get('direction_accepted'),direction_sample_age_ms=round((a.get('now_ns',s['end_ns'])-a.get('direction_sample_end_ns',0))/1e6,3),range_ready=i.get('direct'),range_ack=i.get('accepted'),source_age_ms=a.get('source_age_ms'),sent=i.get('sent'),range_feedback=observed))
        if vector(i.get('sent')):sent.append(tuple(round(x,6) for x in i['sent']))
        if i.get('direction_accepted') and vector(a.get('direction_observed')) and a.get('direction_sample_start_ns',0)>=down:dirs.append(tuple(round(x,5) for x in a['direction_observed']))
    out=dict(probe=str(path),request=p.get('request'),status=p.get('status'),sampled_held=seq,late_held_samples=len(late),late_direction_geometry=sum(bool(s.get('data',{}).get('input',{}).get('direction_fresh')) for s in late),late_direction_ack=sum(bool(s.get('data',{}).get('input',{}).get('direction_accepted')) for s in late),distinct_sent_points=len(set(sent)),distinct_observed_current_cast_directions=len(set(dirs)),confirmed_native_range_vector_error_world=summary(errors),cleanup_contacts_empty=p.get('cleanup_contacts_empty'),release_before_forced_end=p.get('release_before_forced_end'),watch=[])
    out['range_feedback_by_phase']={phase:dict(samples=b['samples'],observed_vector_difference_world=summary(b['observed']),
        post_goal_temporal_difference_world=summary(b['post_goal']),confirmed_current_goal_vector_error_world=summary(b['confirmed']),
        classifications=b['classifications']) for phase,b in feedback.items()}
    released=next((s for s in samples if up and s.get('end_ns',0)>=up and s.get('stage')!='after_forced_end' and release_state(s,movement)),None)
    out['physical_up_to_sampled_release_ms']=(released['end_ns']-up)/1e6 if released and up else None
    out['movement_preserved_at_skill_release']=bool(released and released.get('data',{}).get('input',{}).get('movement_held') and released.get('data',{}).get('input',{}).get('movement_touch') in released.get('data',{}).get('contacts',{}).get('active',[])) if movement else None
    for w in watches:
        if w['hero']!=hero:continue
        before=[r for r in w['receipts'] if down and r['ns']<=down];after=[r for r in w['receipts'] if up and up<=r['ns']<=max(p.get('end_ns',up),up+500_000_000)]
        if not before or not after:continue
        first=before[-1];last=after[-1];delta={k:last['values'].get(k,0)-v for k,v in first['values'].items() if 'Count' in k}
        src=[r for r in w['source'] if down<=r['sample_end_ns']<=up]
        out['watch'].append(dict(path=w['path'],skill=w['skill'],local_actor_counter_delta=delta,before_receipt_ns=first['ns'],after_receipt_ns=last['ns'],after_receipt_delay_ms=(last['ns']-up)/1e6,source_held_samples=len(src),source_valid_drag=sum(bool(r.get('valid_drag')) for r in src),source_aim_visible=sum(r.get('aim_world') is not None for r in src),source_direction_ready=sum(bool(r.get('direction_ready')) for r in src),range_values=sorted(set(r.get('world_range',{}).get('value') for r in src if r.get('world_range',{}).get('kind')==1)),native_range_authorized=sum(bool(r.get('world_range_source_present')) for r in src)))
    if not out['watch']:out['limitations']='No aligned coherent local Actor counter receipt supplied: actual application/cancellation is unproven by this probe alone.'
    out['interpretation']='Immediate and settled vector differences are observations, not confirmed accuracy. Confirmed Range error additionally requires a native receipt starting after the pointer goal acknowledgment and matched_seq==current sent_seq>0; older runtimes without sent_seq cannot establish this. Vectors are world-space/clamped, not screen pixels or target-hit evidence. Sparse held snapshots can miss transient gaps.'
    return out

def main(argv=None):
    ap=argparse.ArgumentParser();ap.add_argument('paths',nargs='+');ap.add_argument('--output');args=ap.parse_args(argv);ws=[watch(p) for p in args.paths if p.endswith('.jsonl')];ps=[p for p in args.paths if p.endswith('.json')];out=[analyze(p,ws) for p in ps];text=json.dumps(out,ensure_ascii=False,indent=2);print(text)
    if args.output:pathlib.Path(args.output).write_text(text+'\n')

if __name__=='__main__':main()
