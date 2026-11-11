#!/usr/bin/env python3
"""Real shared mixer: two rates, callback + queued voice, exactly one Pulse output.
Only an owned null sink/monitor is used; never records a physical microphone.
"""
import ctypes as C,json,math,os,subprocess,sys,threading,time
from pathlib import Path
root=Path(__file__).resolve().parents[1]
if '--child' not in sys.argv:
 name='clbb_mix_test_'+str(os.getpid())
 module=subprocess.check_output(['pactl','load-module','module-null-sink','sink_name='+name,'rate=48000','channels=2'],text=True).strip()
 try:
  subprocess.run([sys.executable,__file__,'--child',name],env={**os.environ,'SDL_AUDIODRIVER':'pulseaudio','PULSE_SINK':name},check=True,timeout=30)
 finally:subprocess.run(['pactl','unload-module',module],check=True)
 sys.exit()
sink=sys.argv[-1]
lib=C.CDLL(str(root/'mapper/runtime/target/release/libclbb_mapper.so'))
Callback=C.CFUNCTYPE(None,C.c_void_p,C.POINTER(C.c_int16),C.c_int)
lib.clbb_mixer_open.argtypes=[C.c_uint,C.c_uint,C.c_uint,C.c_void_p,C.c_void_p];lib.clbb_mixer_open.restype=C.c_uint
lib.clbb_mixer_write.argtypes=[C.c_uint,C.POINTER(C.c_int16),C.c_int];lib.clbb_mixer_write.restype=C.c_int
for name in ['close','clear','pending','available']:getattr(lib,'clbb_mixer_'+name).argtypes=[C.c_uint]
lib.clbb_mixer_active.argtypes=[C.c_uint,C.c_int]
phase=0
@Callback
def game(_,out,size):
 global phase
 for i in range(size//4):
  value=int(6000*math.sin(2*math.pi*440*phase/44100));phase+=1
  out[i*2]=value;out[i*2+1]=value
assert lib.clbb_mixer_open(0,2,0,None,None)==0
assert lib.clbb_mixer_open(16000,99,0,None,None)==0
# Open a paused empty source first; SDL can override PULSE_SINK with its chosen
# default device. Move the exact owned stream before producing any test audio.
voice_id=lib.clbb_mixer_open(16000,1,640,None,None)
assert voice_id
clients=json.loads(subprocess.check_output(['pactl','-f','json','list','clients']))
ids={str(c['index']) for c in clients if str(c.get('properties',{}).get('application.process.id',c.get('properties',{}).get('pipewire.sec.pid','')))==str(os.getpid())}
owned=[s for s in json.loads(subprocess.check_output(['pactl','-f','json','list','sink-inputs'])) if str(s.get('client')) in ids or str(s.get('properties',{}).get('application.process.id',''))==str(os.getpid())]
assert len(owned)==1
subprocess.run(['pactl','move-sink-input',str(owned[0]['index']),sink],check=True)
# Newly added sources must stay on that one existing desktop stream.
game_id=lib.clbb_mixer_open(44100,2,0,C.cast(game,C.c_void_p),None)
assert game_id and game_id!=voice_id
lib.clbb_mixer_active(voice_id,1)
stop=threading.Event();written=0
def feed():
 global written
 phase=0
 while not stop.is_set():
  block=(C.c_int16*320)(*(int(6000*math.sin(2*math.pi*880*(phase+i)/16000)) for i in range(320)))
  count=lib.clbb_mixer_write(voice_id,block,320)
  assert count>=0
  phase+=count;written+=count
thread=threading.Thread(target=feed);thread.start()
rec=subprocess.Popen(['pacat','--record','--device='+sink+'.monitor','--rate=48000','--channels=2','--format=s16le'],stdout=subprocess.PIPE)
try:
 time.sleep(.15)
 def streams():
  clients=json.loads(subprocess.check_output(['pactl','-f','json','list','clients']))
  ids={str(c['index']) for c in clients if str(c.get('properties',{}).get('application.process.id',c.get('properties',{}).get('pipewire.sec.pid','')))==str(os.getpid())}
  return [s for s in json.loads(subprocess.check_output(['pactl','-f','json','list','sink-inputs'])) if str(s.get('client')) in ids or str(s.get('properties',{}).get('application.process.id',''))==str(os.getpid())]
 assert len(streams())==1,streams()
 data=rec.stdout.read(48000*4*3)[-48000*4:]
 assert len(data)==48000*4
 samples=C.cast(C.create_string_buffer(data),C.POINTER(C.c_int16))
 mono=[samples[i*2] for i in range(12000,48000)]
 def level(freq):
  n=len(mono)
  return 2*abs(sum(v*complex(math.cos(2*math.pi*freq*i/48000),math.sin(2*math.pi*freq*i/48000)) for i,v in enumerate(mono)))/n
 a,b=level(440),level(880)
 assert a>2000 and b>2000,(a,b)
 assert written>16000
 stop.set();thread.join(timeout=2);assert not thread.is_alive()
 lib.clbb_mixer_close(voice_id);assert lib.clbb_mixer_available(voice_id)==-3
 # Closing one source cannot close or reopen the desktop device used by the other.
 assert len(streams())==1
 lib.clbb_mixer_close(game_id)
 assert len(streams())==1
 # Exercise source churn without ever creating another system output.
 for i in range(32):
  slot=lib.clbb_mixer_open(48000,2,960,None,None);assert slot
  lib.clbb_mixer_clear(slot);lib.clbb_mixer_close(slot)
 assert len(streams())==1
 print('SHARED_PLAYBACK_OK desktop_outputs=1 callback44100stereo_and_queued16000mono resampled_and_mixed levels=',round(a),round(b),'bounded_writer_frames=',written)
finally:
 stop.set();thread.join(timeout=2)
 lib.clbb_mixer_close(voice_id);lib.clbb_mixer_close(game_id)
 rec.terminate();rec.wait(timeout=3)
