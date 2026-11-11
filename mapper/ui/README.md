# CLBB native settings

Self-authored GTK4 widgets and CSS. No external images, icons, copied game UI,
JavaScript, Qt, JSON parser, or separate settings process is used.

`clbb-settings.h` defines the complete string-key settings contract. The native
adapter supplies `read`, `write`, and `visibility`; the Rust owner validates and
persists settings, applies volume, releases mapped touches, and manages pointer
confinement. The UI never reads game memory or injects input itself.

All callbacks and methods run on the GTK main thread. `read()` may reuse one
scratch buffer after returning: the UI copies text before the next callback.
For an inherited common button, reading `hero.bindings.<slot>.key/cast_mode`
should return the effective common value while `.override` is false. This also
applies to `.stop_movement` for the battle spell. Personal hero skills and
`extra:2` expose an independent stop-movement toggle; tap-only buttons do not.

```c
ClbbSettingsCallbacks callbacks = {
    .read = settings_read,
    .write = settings_write,
    .visibility = settings_visibility,
    .user = state,
};
ClbbSettings *panel = clbb_settings_create(game_overlay, &callbacks);
clbb_settings_show(panel);
/* After a hero change or updated diagnostics, on the main thread: */
clbb_settings_refresh(panel);
/* Before disposing the native adapter: */
clbb_settings_destroy(panel);
```

`clbb_settings_show` reveals a panel inside the game's existing GTK overlay.
Showing it first calls `visibility(true)` so the adapter can stop input and
release confinement before focus moves to the panel. Hiding it restores the
previous focus when possible and calls `visibility(false)` once. The panel fits
the allocated game window and scrolls within it. Use Return to Game, Escape,
or the configured settings key to dismiss it. While assigning a key, Escape cancels,
Backspace clears, and the UI consumes the assignment. Left mouse is never an
assignable control; the settings owner must also reject its numeric code.

`clbb_settings_keycode` maps a GDK keyval into legacy Qt-compatible numeric codes
without a Qt dependency. Physical key capture attempts the Latin group first,
so changing the desktop keyboard layout does not turn Q into a Cyrillic bind.

## Display settings

The Screen card reads `display.width`, `display.height`, and optionally
`display.imported_resolution` (`4000x3111` for the migrated EWM profile).
Presets and the two 256–8192 spin controls edit a local draft. Refreshes do not
replace an unsaved draft. Apply sends one atomic
`write(user, "display.resolution", "WxH")`; the owner changes the logical Android
surface size and keeps the image fitted to its window. Digits and preset
selection never trigger resize writes by themselves.

## FPS and VSync

The Screen card also reads `display.fps_limit` (30–240, default 60) and
`display.vsync` (boolean, default true). Presets are 30, 60, 90, 120, 144, 165,
180 and 240 FPS; the spin control permits intermediate whole values. FPS and
VSync share a separate local draft and Apply FPS button. The only write is
`display.frame_pacing="limit=120;vsync=0"`, so the owner can apply both together.
Pending typed text survives telemetry refreshes and VSync toggles.

This is the runtime's upper frame-rate limit/pacing control. It does not remove
the game's own FPS limits or promise that the requested rate will be reached.
The UI says the effective result depends on the game, performance and display.

The Rust settings owner validates both fields before one save; unknown or
repeated fields, missing values and out-of-range FPS are rejected without
changing either effective value. A failed save rolls both settings back.

## Preview and checks

`check-preview.sh` compiles with warnings as errors and runs a small widget-level
self-test on a private Xvfb server. It exercises capture, normalization, model
refresh, hero rebuilding, overrides, and show/hide callbacks. It cannot change
the real desktop focus or game input.

```sh
mapper/ui/check-preview.sh
mapper/ui/check-preview.sh --screenshots
CLBB_PREVIEW_SCREEN=800x600 mapper/ui/check-preview.sh
CLBB_PREVIEW_SCREEN=640x480 mapper/ui/check-preview.sh
```

Screenshots are generated under `mapper/ui/preview/` on the isolated display.
The optional interactive preview uses an in-memory model and never modifies
real settings:

```sh
mapper/ui/check-preview.sh --run --tab hero
```

Tabs: `main`, `controls`, `hero`, `sound`, `hotkeys`.

## Aim flight recorder

The Hotkeys page contains `debug.recording` (off by default), `debug.auto_marks`
(on by default), and `debug.mark_shortcut` (Ctrl+S). This shortcut alone supports
Ctrl/Shift/Alt combinations; Backspace disables it. Plain S remains a game bind.
A manual mark is also available through the owned diagnostic socket with
`write debug.mark manual` and never releases mapped contacts.

While enabled, Rust retains a rolling five-second buffer in RAM. A mark saves
five seconds before and five seconds after it. Nearby marks share a clip; long
bursts produce overlapping bounded clips. A separate bounded writer queue
serializes JSONL, never the input thread. Nothing is continuously written for
unmarked gameplay. Directory: `$XDG_STATE_HOME/clbb/aim-debug`, normally
`~/.local/state/clbb/aim-debug`. At 256 MiB saving stops with an explicit status;
existing captures are never silently deleted. Starting or disabling recording
mid-window, or hitting row limits, produces an explicitly truncated clip.

Captured data: source read brackets, game clocks/epoch, camera matrices,
hero render/logic coordinates, native skill input/drag flags, indicator,
joystick center/radius, cursor and key transitions, active virtual contact IDs,
command acknowledgements, release goals, and separately bracketed local skill
result fields. These are gameplay telemetry, not a full process-memory dump or
video. Source uses its active two-millisecond cadence; controller/result
observations are at most every eight milliseconds and pointer events at most
every four milliseconds. Key transitions are not throttled. Repeated clocks
are not counted as independent game updates.

Auto marks flag release waits over 100 ms, an active source older than 100 ms,
or a sustained native-aim discrepancy over 150 ms. They are suspicions for
review, not proof of a bug; automatic marks are debounced for two seconds.
