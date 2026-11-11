#!/usr/bin/env python3
"""Compile and exercise the real Java MotionEvent class without game/desktop input."""
from pathlib import Path
import subprocess
import tempfile
root = Path(__file__).resolve().parents[1]
classes = root / 'src/atl/build/src/api-impl/hax.jar.p'
if not (classes / 'android/view/InputEvent.class').is_file():
    raise SystemExit('Build ATL Java classes first (meson compile -C src/atl/build).')
with tempfile.TemporaryDirectory(prefix='clbb-motion-event-') as tmp:
    subprocess.run(['javac', '-nowarn', '-cp', str(classes), '-d', tmp,
                    str(root / 'src/atl/src/api-impl/android/view/MotionEvent.java'),
                    # Recompile Android's real SparseArray for the host JDK;
                    # its ART-built class uses typed System.arraycopy overloads.
                    str(root / 'src/atl/src/api-impl/android/util/SparseArray.java'),
                    str(root / 'smoke/CLBBMotionEventContract.java')], check=True)
    subprocess.run(['java', '-ea', '-cp', tmp+':'+str(classes), 'CLBBMotionEventContract'], check=True, timeout=15)
