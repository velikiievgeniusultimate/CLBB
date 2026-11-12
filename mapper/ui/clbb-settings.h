#ifndef CLBB_SETTINGS_H
#define CLBB_SETTINGS_H
#include <gtk/gtk.h>
G_BEGIN_DECLS

typedef struct ClbbSettings ClbbSettings;
/* read() strings are borrowed until the next callback; UI copies retained text.
 * write() returns FALSE for invalid/reserved binds or a persistence error.
 * All callbacks run on the GTK main thread. Do not inject input from read().
 *
 * Values are UTF-8 strings. Booleans: "true"/"false"; numbers: decimal.
 * audio.input_device / audio.output_device: stable server name or "default".
 * audio.input_devices / audio.output_devices: read-only UTF-8 id TAB label LF rows.
 * audio.refresh: rescan asynchronously; audio.status: routing status.
 * audio.capture_status: counters and state only, no saved voice data.
 * enabled
 * name (default CLBB), status (read-only runtime status)
 * diagnostics.source_ms (read-only sample duration, in milliseconds)
 * movement.button (Qt-style key/mouse code; default 0x02000002 = RMB)
 * hero.movement.<field> / .override: optional per-hero movement field; unchanged
 * fields inherit movement.<field>. Fields: button, click_mode, hold_ms,
 * click_distance_percent, up, down, left, right. Keyboard defaults: WASD. Hero changes reapply the effective movement values.
 * movement.click_mode (0 straight, 1 navigation, 2 keyboard), movement.hold_ms (40..600)
 * audio.volume (0..100), audio.muted
 * display.width / display.height (read-only current sizes, 256..8192)
 * display.imported_resolution (optional saved EWM size as "WxH")
 * display.resolution (atomic write "WxH", only when Apply is clicked)
 * display.fps_limit (30..240, default 60), display.vsync (default true)
 * display.frame_pacing (atomic write "limit=120;vsync=0", on Apply FPS only)
 * hotkeys.settings (default 0x01000036 = F7)
 * hotkeys.confine (default 0x0100003b = F12)
 * hotkeys.fullscreen (default 0x0100003a = F11)
 * debug.recording (default false), debug.auto_marks (default true)
 * debug.mark_shortcut (Qt-style keyboard code | Shift 0x02000000 |
 *   Ctrl 0x04000000 | Alt 0x08000000; default Ctrl+S, 0 disables shortcut)
 * debug.status (read-only), debug.mark (write-only manual marker)
 * hero.id (read-only number), hero.name (read-only display name)
 * hero.cancel_key (default 84 = T)
 *
 * common.bindings.<slot>.key / .cast_mode / .stop_movement
 * hero.bindings.<slot>.key / .cast_mode / .stop_movement / .override
 * cast_mode: 0 release/preview, 1 immediate.
 * stop_movement: boolean for personal hero skills and battle spell extra:2.
 * Slots: skill:0..3; skill:N:ID stores a native form mode/stop flag.
 * All versions of skill:N share its key; unset form fields inherit that role.
 * extra:0 (recall), extra:1 (regen), extra:2 (battle spell), extra:3;
 * attack, attack:minion, attack:tower (one Attack group), shop:quick, upgrade:0..3.
 * Main attack aims only with a current positive native attack-position setting
 * and qualified native attack preview. Normal/unknown attack stays a plain button.
 * An unset key is "0". Duplicate active bindings are shown as conflicts and
 * disabled until resolved (including keyboard movement directions).
 * bindings.conflicts: read-only key-code TAB owner labels LF, active match only.
 * hero.bindings.<slot>.<field>.override: per-field boolean. Editing a personal
 * field enables its override; writing false resets that field to the common
 * value. Unchanged neighboring fields continue inheriting common edits.
 * hero.controls: read-only, current owned native UI layout; empty outside a
 * fresh match. Header: 1 TAB heroId LF. Rows: native slot TAB config slot TAB
 * mode-config slot TAB base skill ID TAB current skill ID TAB type LF.
 * Repeated native slots with different config slots describe versions inside
 * the same skill card, not additional buttons. Only present buttons are listed; cooldown does not
 * itself remove a button. The Hero page is built from this bounded layout.
 * LMB (0x02000001) must always be rejected by the settings owner.
 * .cast_mode is not offered for attack, upgrade, shop, recall or regeneration.
 */
typedef struct {
  const char *(*read)(void *user, const char *key);
  gboolean (*write)(void *user, const char *key, const char *value);
  void (*visibility)(void *user, gboolean visible);
  void *user;
} ClbbSettingsCallbacks;

ClbbSettings *clbb_settings_create(GtkOverlay *overlay,
                                   const ClbbSettingsCallbacks *callbacks);
void clbb_settings_show(ClbbSettings *settings);
void clbb_settings_hide(ClbbSettings *settings);
void clbb_settings_refresh(ClbbSettings *settings);
gboolean clbb_settings_is_capturing(const ClbbSettings *settings);
gboolean clbb_settings_is_visible(const ClbbSettings *settings);
GtkWidget *clbb_settings_panel(ClbbSettings *settings);
void clbb_settings_destroy(ClbbSettings *settings);
/* Also used by the native input adapter. Returns 0 for an unsupported key. */
gint clbb_settings_keycode(guint keyval);
/* Stable binding code for a GDK button; preserves existing Qt mouse masks.
 * GDK2/3 -> middle/right; GDK8..29 -> legacy one-bit masks. Other buttons
 * 4..7,30..65535 use 0x03000000|GDKbutton. Primary LMB remains unbindable.
 * Both capture and native gameplay delivery must use this same conversion. */
gint clbb_settings_mousecode(guint button);
gint clbb_settings_chord(gint code, GdkModifierType state);
G_END_DECLS
#endif
