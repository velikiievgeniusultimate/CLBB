#!/usr/bin/env python3
"""Owned synthetic Pulse source only: no desktop default changes, no real voice saved."""
import math, os, struct, subprocess, threading, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
name=f'clbb_audio_test_{os.getpid()}'
def pactl(*args):return subprocess.check_output(['pactl',*args],text=True).strip()
module=pactl('load-module','module-null-sink',f'sink_name={name}','rate=48000','channels=1')
module_b=None
player=None
try:
    second=name+'_b'
    module_b=pactl('load-module','module-null-sink',f'sink_name={second}','rate=48000','channels=1')
    subprocess.run(['cargo','test','--manifest-path',str(ROOT/'mapper/runtime/Cargo.toml'),'--lib','audio::routing_tests::own_streams_move_without_changing_defaults','--','--ignored','--nocapture'],env={**os.environ,'CLBB_TEST_AUDIO_SINK_A':name,'CLBB_TEST_AUDIO_SINK_B':second},check=True,timeout=180)
    player=subprocess.Popen(['pacat','--playback',f'--device={name}','--rate=48000','--channels=1','--format=s16le'],stdin=subprocess.PIPE,bufsize=0)
    pcm=b''.join(struct.pack('<h',int(8000*math.sin(2*math.pi*440*i/48000))) for i in range(4800))
    def feed():
        try:
            while player.poll() is None:
                player.stdin.write(pcm);player.stdin.flush()
        except (BrokenPipeError,ValueError):pass
    threading.Thread(target=feed,daemon=True).start()
    subprocess.run(['cargo','test','--manifest-path',str(ROOT/'mapper/runtime/Cargo.toml'),'--lib','audio::capture_tests::synthetic_capture','--','--ignored','--nocapture'],env={**os.environ,'CLBB_TEST_AUDIO_SOURCE':name+'.monitor'},check=True,timeout=180)
finally:
    if player:player.terminate();player.wait(timeout=5)
    if module_b:pactl('unload-module',module_b)
    pactl('unload-module',module)
