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
 * enabled
 * name (default CLBB), status (read-only runtime status)
 * diagnostics.source_ms (read-only sample duration, in milliseconds)
 * movement.button (Qt-style key/mouse code; default 0x02000002 = RMB)
 * movement.click_mode (0 straight, 1 navigation), movement.hold_ms (40..600)
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
 * Slots: skill:0, skill:1, skill:1:2521, skill:2, skill:3;
 * extra:0 (recall), extra:1 (regen), extra:2 (battle spell), extra:3;
 * attack, attack:minion, attack:tower, shop:quick, upgrade:0..3.
 * An unset key is "0". Duplicate hero bindings are permitted.
 * Global slots have a per-hero override switch; disabled means common value.
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
gint clbb_settings_chord(gint code, GdkModifierType state);
G_END_DECLS
#endif
