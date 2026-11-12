/* Development preview only. It uses an in-memory model and never opens the
 * game process, reads game memory, grabs a pointer, or modifies user settings.
 * Run on a separate Xvfb display to inspect without focusing the game. */
#include "clbb-settings.h"
#include <string.h>

typedef struct {
  GHashTable *values;
  GMainLoop *loop;
  guint writes;
  guint visible;
  char *layout;
} Preview;
static const char *read_value(void *data, const char *key) {
  Preview *p = data;
  if (!strcmp(key,"hero.controls")) {
    const char *custom=g_hash_table_lookup(p->values,"preview.controls");
    if (custom) return custom;
    int hero=atoi(g_hash_table_lookup(p->values,"hero.id")?g_hash_table_lookup(p->values,"hero.id"):"0");
    g_free(p->layout);p->layout=NULL;
    if (hero<=0) return "";
    GString *rows=g_string_new(NULL);g_string_append_printf(rows,"1\t%d\n",hero);
    gboolean linked=!g_strcmp0(g_hash_table_lookup(p->values,"hero.linked_controls"),"1");
    for (int n=0;n<(linked?2:3);++n) {
      char *key=g_strdup_printf("hero.skill%d_type",n);const char *type=g_hash_table_lookup(p->values,key);
      g_string_append_printf(rows,"skill:%d\tskill:%d\tskill:%d\t%d\t%d\t%s\n",n,n,n,hero*100+n*10+10,hero*100+n*10+10,type?type:n==2?"1":"2");g_free(key);
    }
    const char *slots[]={"extra:0","extra:1","extra:2","extra:3","attack","attack:minion","attack:tower","shop:quick","upgrade:0","upgrade:1","upgrade:2"};
    for (guint n=0;n<G_N_ELEMENTS(slots);++n) g_string_append_printf(rows,"%s\t%s\t%s\t%d\t%d\t%d\n",slots[n],slots[n],slots[n],n==2?20020:0,n==2?20020:0,n==2?2:1);
    if (linked) for (int n=0;n<2;++n) g_string_append_printf(rows,"combo:%d\tcombo:%d\tcombo:%d\t0\t0\t1\n",n,n,n);
    p->layout=g_string_free(rows,FALSE);return p->layout;
  }
  if (g_str_has_prefix(key,"hero.movement.")&&!g_str_has_suffix(key,".override")) {
    char *marker=g_strconcat(key,".override",NULL);
    gboolean own=!g_strcmp0(g_hash_table_lookup(p->values,marker),"true");g_free(marker);
    if (!own) return g_hash_table_lookup(p->values,key+5);
  }
  if (g_str_has_prefix(key,"hero.bindings.")) {
    const char *dot=strrchr(key,'.');
    if (dot&&(!strcmp(dot,".key")||!strcmp(dot,".cast_mode")||!strcmp(dot,".stop_movement"))) {
      char *marker=g_strconcat(key,".override",NULL);
      gboolean own=!g_strcmp0(g_hash_table_lookup(p->values,marker),"true");g_free(marker);
      if (!own) {
        char *common=g_strconcat("common.",key+5,NULL);
        const char *v=g_hash_table_lookup(p->values,common);g_free(common);return v;
      }
    }
  }
  return g_hash_table_lookup(p->values, key);
}
static gboolean write_value(void *data, const char *key, const char *value) {
  Preview *p = data;
  if ((g_str_has_suffix(key, ".key") || g_str_has_prefix(key, "hotkeys.") ||
       !strcmp(key, "movement.button")) &&
      g_ascii_strtoll(value, NULL, 0) == 0x02000001)
    return FALSE;
  if (g_str_has_prefix(key, "hotkeys.")) {
    const char *slots[] = {"hotkeys.settings", "hotkeys.confine",
                           "hotkeys.fullscreen"};
    for (guint i = 0; i < G_N_ELEMENTS(slots); ++i)
      if (strcmp(slots[i], key) && g_ascii_strtoll(value, NULL, 0) &&
          g_ascii_strtoll(value, NULL, 0) ==
              g_ascii_strtoll(read_value(p, slots[i]) ? read_value(p, slots[i])
                                                      : "0",
                              NULL, 0))
        return FALSE;
  }
  if (!strcmp(key, "display.resolution")) {
    char **size = g_strsplit(value, "x", -1);
    char *end_w = NULL, *end_h = NULL;
    gint64 w = size[0] ? g_ascii_strtoll(size[0], &end_w, 10) : 0;
    gint64 h = size[1] ? g_ascii_strtoll(size[1], &end_h, 10) : 0;
    gboolean valid = g_strv_length(size) == 2 && end_w && !*end_w && end_h &&
                     !*end_h && w >= 256 && w <= 8192 && h >= 256 && h <= 8192;
    if (!valid) {
      g_strfreev(size);
      return FALSE;
    }
    g_hash_table_replace(p->values, g_strdup("display.width"),
                         g_strdup(size[0]));
    g_hash_table_replace(p->values, g_strdup("display.height"),
                         g_strdup(size[1]));
    g_strfreev(size);
  }
  if (!strcmp(key, "display.frame_pacing")) {
    char **fields = g_strsplit(value, ";", -1);
    gboolean valid =
        g_strv_length(fields) == 2 && g_str_has_prefix(fields[0], "limit=") &&
        (!strcmp(fields[1], "vsync=0") || !strcmp(fields[1], "vsync=1"));
    char *end = NULL;
    gint64 fps = valid ? g_ascii_strtoll(fields[0] + 6, &end, 10) : 0;
    valid = valid && end && !*end && fps >= 30 && fps <= 240;
    if (!valid) {
      g_strfreev(fields);
      return FALSE;
    }
    g_hash_table_replace(p->values, g_strdup("display.fps_limit"),
                         g_strdup(fields[0] + 6));
    g_hash_table_replace(
        p->values, g_strdup("display.vsync"),
        g_strdup(!strcmp(fields[1], "vsync=1") ? "true" : "false"));
    g_strfreev(fields);
  }
  g_hash_table_replace(p->values, g_strdup(key), g_strdup(value));
  if ((g_str_has_prefix(key,"hero.movement.")&&!g_str_has_suffix(key,".override")) || (g_str_has_prefix(key,"hero.bindings.")&&(g_str_has_suffix(key,".key")||g_str_has_suffix(key,".cast_mode")||g_str_has_suffix(key,".stop_movement")))) {
    char *marker=g_strconcat(key,".override",NULL);
    g_hash_table_replace(p->values,marker,g_strdup("true"));
  }
  p->writes++;
  return TRUE;
}
static void visibility(void *data, gboolean visible) {
  Preview *p = data;
  p->visible += visible ? 1 : 0;
}
static void visible_changed(GObject *object, GParamSpec *spec, gpointer data) {
  (void)spec;
  Preview *p = data;
  if (!gtk_widget_get_visible(GTK_WIDGET(object)))
    g_main_loop_quit(p->loop);
}
static gboolean quit_later(gpointer data) {
  g_main_loop_quit(data);
  return G_SOURCE_REMOVE;
}
static GtkWidget *find_type(GtkWidget *root, GType type) {
  if (g_type_is_a(G_OBJECT_TYPE(root), type))
    return root;
  for (GtkWidget *w = gtk_widget_get_first_child(root); w;
       w = gtk_widget_get_next_sibling(w)) {
    GtkWidget *r = find_type(w, type);
    if (r)
      return r;
  }
  return NULL;
}
static GtkWidget *find_button(GtkWidget *root, const char *text) {
  if (GTK_IS_BUTTON(root) &&
      !g_strcmp0(gtk_button_get_label(GTK_BUTTON(root)), text))
    return root;
  for (GtkWidget *w = gtk_widget_get_first_child(root); w;
       w = gtk_widget_get_next_sibling(w)) {
    GtkWidget *r = find_button(w, text);
    if (r)
      return r;
  }
  return NULL;
}
static GtkWidget *find_control(GtkWidget *root, const char *key) {
  const char *row_key = g_object_get_data(G_OBJECT(root), "clbb-settings-key");
  if (row_key && !strcmp(row_key, key))
    return root;
  for (GtkWidget *w = gtk_widget_get_first_child(root); w;
       w = gtk_widget_get_next_sibling(w)) {
    GtkWidget *result = find_control(w, key);
    if (result)
      return result;
  }
  return NULL;
}
static void put(Preview *p, const char *k, const char *v) {
  g_hash_table_replace(p->values, g_strdup(k), g_strdup(v));
}
int main(int argc, char **argv) {
  gtk_init();
  Preview p = {
      .values = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, g_free),
      .loop = g_main_loop_new(NULL, FALSE)};
  put(&p, "hero.id", "25");
  put(&p, "hero.name", "Кагура");
  put(&p, "status", "Матч активен · профиль Кагуры");
  put(&p, "diagnostics.source_ms", "0.12");
  put(&p, "hotkeys.settings", "16777270");
  put(&p, "hotkeys.confine", "16777275");
  put(&p, "hotkeys.fullscreen", "16777274");
  put(&p, "common.bindings.attack.key", "32");
  put(&p, "common.bindings.extra:2.key", "70");
  put(&p, "audio.volume", "72");
  put(&p, "audio.game_volume", "100");
  put(&p, "audio.voice_volume", "100");
  put(&p,"audio.input_device","default");put(&p,"audio.output_device","default");
  put(&p,"audio.input_devices","default\tDefault\nmic.one\tМикрофон гарнитуры\nmic.two\tВнешний микрофон\n");
  put(&p,"audio.output_devices","default\tDefault\nsink.one\tНаушники\nsink.two\tКолонки\n");
  put(&p,"audio.status","Устройства подключены");
  put(&p, "display.width", "1280");
  put(&p, "display.height", "720");
  put(&p, "display.imported_resolution", "4000x3111");
  put(&p, "display.fps_limit", "60");
  put(&p, "display.vsync", "true");
  put(&p, "updates.version", "v0.1.0-alpha.1");
  put(&p, "updates.status", "Доступна новая версия CLBB.");
  put(&p, "updates.available", "true");
  ClbbSettingsCallbacks callbacks = {read_value, write_value, visibility, &p};
  GtkWindow *preview = GTK_WINDOW(gtk_window_new());
  gtk_window_set_title(preview, "CLBB · Настройки");
  GdkDisplay *display = gtk_widget_get_display(GTK_WIDGET(preview));
  GdkMonitor *monitor = g_list_model_get_item(gdk_display_get_monitors(display), 0);
  GdkRectangle area;
  gdk_monitor_get_geometry(monitor, &area);
  g_object_unref(monitor);
  gtk_window_set_default_size(preview, MIN(1060, area.width - 32),
                              MIN(760, area.height - 64));
  GtkWidget *overlay = gtk_overlay_new();
  gtk_window_set_child(preview, overlay);
  ClbbSettings *s = clbb_settings_create(GTK_OVERLAY(overlay), &callbacks);
  gboolean selftest = FALSE;
  gboolean linked_hero = FALSE;
  gboolean expand_first = FALSE;
  const char *tab = "main";
  guint timeout = 0;
  for (int i = 1; i < argc; ++i) {
    if (!strcmp(argv[i], "--self-test"))
      selftest = TRUE;
    else if (!strcmp(argv[i], "--expand-first")) expand_first=TRUE;
    else if (!strcmp(argv[i], "--linked-hero"))
      linked_hero = TRUE;
    else if (!strcmp(argv[i], "--tab") && i + 1 < argc)
      tab = argv[++i];
    else if (!strcmp(argv[i], "--timeout") && i + 1 < argc)
      timeout = (guint)g_ascii_strtoull(argv[++i], NULL, 10);
  }
  if (linked_hero) {
    put(&p, "hero.id", "133");put(&p, "hero.name", "Хирара");
    put(&p, "hero.linked_controls", "1");put(&p, "hero.skill0_type", "1");
    put(&p, "hero.bindings.combo:0.key", "69");put(&p, "hero.bindings.combo:1.key", "82");
    clbb_settings_refresh(s);
  }
  if (selftest) {
    GHashTable *mouse_codes = g_hash_table_new(g_direct_hash, g_direct_equal);
    for (guint button = 1; button <= 255; ++button) {
      gint code = clbb_settings_mousecode(button);
      g_assert_cmpint(code, >, 0);
      g_assert_false(g_hash_table_contains(mouse_codes, GINT_TO_POINTER(code)));
      g_hash_table_add(mouse_codes, GINT_TO_POINTER(code));
      g_assert_cmpint(0x02000000 | (code & ~0x02000000), ==, code);
    }
    g_assert_cmpint(clbb_settings_mousecode(8), ==, 0x02000008);
    g_assert_cmpint(clbb_settings_mousecode(9), ==, 0x02000010);
    g_assert_cmpint(clbb_settings_mousecode(10), ==, 0x02000020);
    g_assert_cmpint(clbb_settings_mousecode(0), ==, 0);
    g_hash_table_unref(mouse_codes);
    GtkWidget *stack = find_type(clbb_settings_panel(s), GTK_TYPE_STACK);
    g_assert_nonnull(gtk_stack_get_child_by_name(GTK_STACK(stack), "controls"));
    g_assert_null(gtk_stack_get_child_by_name(GTK_STACK(stack), "hero"));
    GtkWidget *skill_row = find_control(clbb_settings_panel(s), "hero.bindings.skill:0.key");
    GtkWidget *common_row = find_control(clbb_settings_panel(s), "common.bindings.extra:2.key");
    GtkWidget *controls_page = gtk_stack_get_child_by_name(GTK_STACK(stack), "controls");
    g_assert_true(gtk_widget_is_ancestor(skill_row, controls_page));
    g_assert_true(gtk_widget_is_ancestor(common_row, controls_page));
    GtkWidget *group = gtk_widget_get_ancestor(skill_row, GTK_TYPE_REVEALER);
    g_assert_nonnull(group);
    GtkWidget *heading = gtk_widget_get_first_child(gtk_widget_get_parent(group));
    g_assert_false(gtk_revealer_get_reveal_child(GTK_REVEALER(group)));
    guint before_expand = p.writes;
    gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(heading), TRUE);
    clbb_settings_refresh(s);
    g_assert_true(gtk_revealer_get_reveal_child(GTK_REVEALER(group)));
    GtkWidget *capture_button = find_type(skill_row, GTK_TYPE_BUTTON);
    g_signal_emit_by_name(capture_button, "clicked");
    g_assert_true(clbb_settings_is_capturing(s));
    gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(heading), FALSE);
    g_assert_false(clbb_settings_is_capturing(s));
    g_assert_cmpuint(p.writes, ==, before_expand);
    // Changing one personal field must leave neighboring fields inherited.
    g_assert(write_value(&p,"common.bindings.skill:0.key","81"));
    g_assert(write_value(&p,"common.bindings.skill:0.cast_mode","1"));
    g_assert(write_value(&p,"hero.bindings.skill:0.key","88"));
    g_assert(write_value(&p,"common.bindings.skill:0.key","89"));
    clbb_settings_refresh(s);
    g_assert_cmpstr(read_value(&p,"hero.bindings.skill:0.key"),==,"88");
    g_assert_cmpstr(read_value(&p,"hero.bindings.skill:0.cast_mode"),==,"1");
    GtkWidget *reset=gtk_widget_get_last_child(find_control(clbb_settings_panel(s),"hero.bindings.skill:0.key"));
    g_assert_true(gtk_widget_get_sensitive(reset));g_signal_emit_by_name(reset,"clicked");
    g_assert_cmpstr(read_value(&p,"hero.bindings.skill:0.key"),==,"89");
    g_assert_false(gtk_widget_get_sensitive(reset));
    put(&p,"common.bindings.skill:0.key","81");
    clbb_settings_refresh(s);
    GtkWidget *attack_reveal=gtk_widget_get_ancestor(find_control(clbb_settings_panel(s),"hero.bindings.attack.key"),GTK_TYPE_REVEALER);
    g_assert_nonnull(attack_reveal);
    g_assert_true(attack_reveal==gtk_widget_get_ancestor(find_control(clbb_settings_panel(s),"hero.bindings.attack:minion.key"),GTK_TYPE_REVEALER));
    g_assert_true(attack_reveal==gtk_widget_get_ancestor(find_control(clbb_settings_panel(s),"hero.bindings.attack:tower.key"),GTK_TYPE_REVEALER));
    // Keyboard controls are mirrored, editable and visible only in keyboard mode.
    GtkWidget *mode_row=find_control(clbb_settings_panel(s),"movement.click_mode");
    GtkWidget *mode=find_type(mode_row,GTK_TYPE_DROP_DOWN);
    g_assert_cmpuint(g_list_model_get_n_items(gtk_drop_down_get_model(GTK_DROP_DOWN(mode))),==,3);
    gtk_drop_down_set_selected(GTK_DROP_DOWN(mode),2);
    g_assert_cmpstr(read_value(&p,"movement.click_mode"),==,"2");
    g_assert_true(gtk_widget_get_visible(find_control(clbb_settings_panel(s),"movement.up")));
    g_assert_false(gtk_widget_get_visible(find_control(clbb_settings_panel(s),"movement.button")));
    put(&p,"bindings.conflicts","87\tНавык 2, движение\n");
    clbb_settings_refresh(s);
    GtkWidget *up_button=find_type(find_control(clbb_settings_panel(s),"movement.up"),GTK_TYPE_BUTTON);
    g_assert_true(gtk_widget_has_css_class(up_button,"clbb-conflict"));
    put(&p,"bindings.conflicts","");clbb_settings_refresh(s);
    g_assert_false(gtk_widget_has_css_class(up_button,"clbb-conflict"));
    gtk_drop_down_set_selected(GTK_DROP_DOWN(mode),0);
    // Alternate forms stay in one skill card with one key and paired press/stop fields.
    put(&p,"preview.controls","1\t25\nskill:1\tskill:1\tskill:1\t2520\t2520\t2\nskill:1\tskill:1:2521\tskill:1:2521\t2520\t2521\t1\nskill:2\tskill:2\tskill:2\t2530\t2530\t1\nskill:2\tskill:2:2531\tskill:2:2531\t2530\t2531\t1\n");
    clbb_settings_refresh(s);
    g_assert_nonnull(find_control(clbb_settings_panel(s),"hero.bindings.skill:1.key"));
    g_assert_null(find_control(clbb_settings_panel(s),"hero.bindings.skill:1:2521.key"));
    g_assert_nonnull(find_control(clbb_settings_panel(s),"hero.bindings.skill:1.cast_mode"));
    g_assert_nonnull(find_control(clbb_settings_panel(s),"hero.bindings.skill:1:2521.cast_mode"));
    g_assert_nonnull(find_control(clbb_settings_panel(s),"common.bindings.skill:2:2531.stop_movement"));
    g_hash_table_remove(p.values,"preview.controls");clbb_settings_refresh(s);
    g_print("CLBB_KEYBOARD_CONFLICT_AND_FORM_UI_OK keyboard visibility, collision warning, nested form settings\n");
    // Absent controls are removed, rather than left as disabled phantom rows.
    put(&p,"preview.controls","1\t25\nskill:0\tskill:0\tskill:0\t2510\t2510\t3\n");
    clbb_settings_refresh(s);
    g_assert_nonnull(find_control(clbb_settings_panel(s),"hero.bindings.skill:0.key"));
    g_assert_null(find_control(clbb_settings_panel(s),"hero.bindings.extra:3.key"));
    g_assert_null(find_control(clbb_settings_panel(s),"hero.bindings.skill:3.key"));
    put(&p,"preview.controls","");clbb_settings_refresh(s);
    g_assert_null(find_control(clbb_settings_panel(s),"hero.bindings.skill:0.key"));
    g_hash_table_remove(p.values,"preview.controls");clbb_settings_refresh(s);
    g_print("CLBB_HERO_LIVE_LAYOUT_OK present controls only, field inheritance, reset, stale data hides rows\n");
    g_print("CLBB_COLLAPSIBLE_CONTROLS_OK merged page, initially collapsed, refresh preserves state, no settings writes\n");
    const char *volume_keys[]={"audio.volume","audio.game_volume","audio.voice_volume"};
    for (guint i=0;i<G_N_ELEMENTS(volume_keys);i++) {
      GtkWidget *row=find_control(clbb_settings_panel(s),volume_keys[i]);
      GtkWidget *slider=find_type(row,GTK_TYPE_SCALE);g_assert_nonnull(slider);
      gtk_range_set_value(GTK_RANGE(slider),20+i*10);
      char expected[16];g_snprintf(expected,sizeof(expected),"%u",20+i*10);
      g_assert_cmpstr(read_value(&p,volume_keys[i]),==,expected);
    }
    guint volume_writes=p.writes;clbb_settings_refresh(s);
    g_assert_cmpuint(p.writes,==,volume_writes);
    GtkWidget *mic=find_control(clbb_settings_panel(s),"audio.input_device");
    GtkWidget *speaker=find_control(clbb_settings_panel(s),"audio.output_device");
    g_assert_nonnull(mic);g_assert_nonnull(speaker);
    gtk_drop_down_set_selected(GTK_DROP_DOWN(mic),2);
    g_assert_cmpstr(read_value(&p,"audio.input_device"),==,"mic.two");
    gtk_drop_down_set_selected(GTK_DROP_DOWN(speaker),1);
    g_assert_cmpstr(read_value(&p,"audio.output_device"),==,"sink.one");
    guint audio_writes=p.writes;clbb_settings_refresh(s);
    g_assert_cmpuint(p.writes,==,audio_writes);
    put(&p,"audio.input_devices","default\tDefault\nmic.one\tМикрофон гарнитуры\n");
    clbb_settings_refresh(s);
    g_assert_cmpuint(gtk_drop_down_get_selected(GTK_DROP_DOWN(mic)),==,2);
    g_assert_cmpstr(gtk_string_list_get_string(GTK_STRING_LIST(gtk_drop_down_get_model(GTK_DROP_DOWN(mic))),2),==,"Недоступно: mic.two");
    g_assert_cmpstr(read_value(&p,"audio.input_device"),==,"mic.two");
    g_assert_cmpuint(p.writes,==,audio_writes);
    g_signal_emit_by_name(find_button(clbb_settings_panel(s),"Обновить устройства"),"clicked");
    g_assert_cmpstr(read_value(&p,"audio.refresh"),==,"1");

    GtkWidget *record_row = find_control(clbb_settings_panel(s), "debug.recording");
    GtkWidget *record_button = find_type(record_row, GTK_TYPE_TOGGLE_BUTTON);
    g_assert_nonnull(record_button);
    for (int cycle = 0; cycle < 20; ++cycle) {
      gboolean active = cycle % 2 == 0;
      gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(record_button), active);
      g_assert_cmpstr(read_value(&p, "debug.recording"), ==, active ? "true" : "false");
      g_assert_cmpstr(gtk_button_get_label(GTK_BUTTON(record_button)), ==,
                     active ? "ВКЛ · запись" : "ВЫКЛ · запись");
      guint writes = p.writes;
      clbb_settings_refresh(s);
      g_assert_cmpuint(p.writes, ==, writes);
      g_assert_cmpint(gtk_toggle_button_get_active(GTK_TOGGLE_BUTTON(record_button)), ==, active);
    }
    g_assert_cmpint(clbb_settings_keycode(GDK_KEY_F7), ==, 0x01000036);
    g_assert_cmpint(clbb_settings_keycode(GDK_KEY_F12), ==, 0x0100003b);
    g_assert_cmpint(clbb_settings_keycode(GDK_KEY_q), ==, 'Q');
    g_assert_cmpint(clbb_settings_chord('S',GDK_CONTROL_MASK),==,0x04000053);
    g_assert_cmpint(clbb_settings_chord('S',GDK_CONTROL_MASK|GDK_SHIFT_MASK),==,0x06000053);
    GtkWidget *debug_key = find_button(GTK_WIDGET(clbb_settings_panel(s)), "Ctrl+S");
    g_assert_nonnull(debug_key);
    g_signal_emit_by_name(debug_key,"clicked");
    GListModel *debug_controllers=gtk_widget_observe_controllers(GTK_WIDGET(clbb_settings_panel(s)));
    gboolean debug_handled=FALSE;
    for(guint i=0;i<g_list_model_get_n_items(debug_controllers);i++) {
      GObject *c=g_list_model_get_item(debug_controllers,i);
      if(GTK_IS_EVENT_CONTROLLER_KEY(c)) g_signal_emit_by_name(c,"key-pressed",GDK_KEY_k,0,GDK_CONTROL_MASK|GDK_SHIFT_MASK,&debug_handled);
      g_object_unref(c);
    }
    g_object_unref(debug_controllers);
    g_assert_cmpstr(read_value(&p,"debug.mark_shortcut"),==,"100663371");
    g_assert(find_button(GTK_WIDGET(clbb_settings_panel(s)),"Ctrl+Shift+K"));
    g_assert_cmpint(clbb_settings_keycode(GDK_KEY_Shift_L), ==, 0x1000ffe1);
    for(guint f=GDK_KEY_F1;f<=GDK_KEY_F35;f++) {
      g_assert_cmpint(clbb_settings_keycode(f),==,0x01000030+(int)(f-GDK_KEY_F1));
      // Hardware F1 must never turn the semantic F13 event back into F1.
      g_assert_cmpint(clbb_settings_event_keycode(gdk_display_get_default(),f,67),==,clbb_settings_keycode(f));
    }
    g_assert_cmpint(clbb_settings_keycode(GDK_KEY_AudioMute),==,0x1008ff12);
    g_assert_cmpint(clbb_settings_event_keycode(gdk_display_get_default(),GDK_KEY_VoidSymbol,250),==,0x200000fa);
    g_assert(find_button(GTK_WIDGET(clbb_settings_panel(s)), "Q"));
    g_assert(find_button(GTK_WIDGET(clbb_settings_panel(s)), "F7"));
    GtkWidget *q = find_type(find_control(clbb_settings_panel(s),"hero.bindings.skill:0.key"),GTK_TYPE_BUTTON);
    g_signal_emit_by_name(q, "clicked");
    GListModel *controllers =
        gtk_widget_observe_controllers(GTK_WIDGET(clbb_settings_panel(s)));
    gboolean handled = FALSE;
    for (guint n = 0; n < g_list_model_get_n_items(controllers); ++n) {
      GObject *controller = g_list_model_get_item(controllers, n);
      if (GTK_IS_EVENT_CONTROLLER_KEY(controller))
        g_signal_emit_by_name(controller, "key-pressed", GDK_KEY_j, 0, 0,
                              &handled);
      g_object_unref(controller);
    }
    g_object_unref(controllers);
    g_assert_cmpstr(read_value(&p, "hero.bindings.skill:0.key"), ==, "74");
    for (guint symbol=GDK_KEY_F13;symbol<=GDK_KEY_F35;symbol++) {
      GtkWidget *binding=find_type(find_control(clbb_settings_panel(s),"hero.bindings.skill:0.key"),GTK_TYPE_BUTTON);
      g_signal_emit_by_name(binding,"clicked");
      GListModel *capture=gtk_widget_observe_controllers(GTK_WIDGET(clbb_settings_panel(s)));
      gboolean caught=FALSE;
      for(guint i=0;i<g_list_model_get_n_items(capture);i++) {
        GObject *controller=g_list_model_get_item(capture,i);
        if(GTK_IS_EVENT_CONTROLLER_KEY(controller))g_signal_emit_by_name(controller,"key-pressed",symbol,67,0,&caught);
        g_object_unref(controller);
      }
      g_object_unref(capture);
      g_assert_true(caught);
      g_assert_cmpint(atoi(read_value(&p,"hero.bindings.skill:0.key")),==,clbb_settings_keycode(symbol));
    }
    g_assert(!write_value(&p, "hero.bindings.skill:0.key", "33554433"));
    g_assert(!write_value(&p, "hotkeys.settings", "16777275"));
    g_assert(write_value(&p, "hero.bindings.skill:0.key", "88"));
    clbb_settings_refresh(s);
    g_assert(find_button(GTK_WIDGET(clbb_settings_panel(s)), "X"));
    put(&p, "hero.id", "1");
    put(&p, "hero.name", "Мия");
    clbb_settings_refresh(s);
    g_assert_null(find_control(clbb_settings_panel(s),"hero.bindings.skill:3.key"));
    put(&p, "hero.id", "25");
    put(&p, "hero.name", "Кагура");
    clbb_settings_refresh(s);
    g_assert(find_button(GTK_WIDGET(clbb_settings_panel(s)), "W"));
    g_assert(write_value(&p, "hero.bindings.extra:2.key", "86"));
    clbb_settings_refresh(s);
    g_assert(find_button(GTK_WIDGET(clbb_settings_panel(s)), "V"));
    GtkWidget *window = GTK_WIDGET(clbb_settings_panel(s));
    GtkWidget *update_open=find_button(window,"Обновить CLBB");
    g_assert(update_open);
    g_signal_emit_by_name(update_open,"clicked");
    g_assert_cmpstr(read_value(&p,"updates.open"),==,"1");
    g_assert(!find_button(window,"Установить обновление"));
    g_print("CLBB_UPDATES_UI_OK main-page handoff request\n");
    GtkWidget *distance_row = find_control(window, "movement.click_distance_percent");
    GtkWidget *distance_spin = find_type(distance_row, GTK_TYPE_SPIN_BUTTON);
    GtkWidget *movement_mode = find_type(find_control(window, "movement.click_mode"), GTK_TYPE_DROP_DOWN);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(distance_spin)), ==, 100);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(distance_spin), 200);
    g_assert_cmpstr(read_value(&p, "movement.click_distance_percent"), ==, "200");
    gtk_drop_down_set_selected(GTK_DROP_DOWN(movement_mode), 1);
    g_assert_false(gtk_widget_get_visible(distance_row));
    gtk_drop_down_set_selected(GTK_DROP_DOWN(movement_mode), 0);
    g_assert_true(gtk_widget_get_visible(distance_row));
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(distance_spin)), ==, 200);
    const char *stops[] = {"hero.bindings.skill:0.stop_movement",
                           "hero.bindings.skill:1.stop_movement",
                           "hero.bindings.skill:2.stop_movement",
                           "common.bindings.extra:2.stop_movement",
                           "hero.bindings.extra:2.stop_movement"};
    for (guint n = 0; n < G_N_ELEMENTS(stops); ++n) {
      GtkWidget *row = find_control(window, stops[n]);
      g_assert_nonnull(row);
      GtkWidget *toggle = find_type(row, GTK_TYPE_SWITCH);
      g_assert_nonnull(toggle);
      g_assert_true(gtk_widget_get_sensitive(toggle));
      gtk_switch_set_active(GTK_SWITCH(toggle), TRUE);
      g_assert_cmpstr(g_hash_table_lookup(p.values, stops[n]), ==, "true");
    }
    g_assert_null(find_control(window, "common.bindings.attack.stop_movement"));
    g_assert_null(
        find_control(window, "common.bindings.extra:0.stop_movement"));
    g_assert_null(
        find_control(window, "common.bindings.shop:quick.stop_movement"));
    g_assert(write_value(&p, "hero.bindings.extra:2.stop_movement.override", "false"));
    clbb_settings_refresh(s);
    GtkWidget *spell_stop =
        find_type(find_control(window, "hero.bindings.extra:2.stop_movement"),
                  GTK_TYPE_SWITCH);
    g_assert_true(gtk_widget_get_sensitive(spell_stop));
    GtkWidget *presets = find_control(window, "display.preset");
    GtkWidget *width = find_control(window, "display.width");
    GtkWidget *height = find_control(window, "display.height");
    GtkWidget *apply = find_button(window, "Применить");
    g_assert_nonnull(presets);
    g_assert_nonnull(width);
    g_assert_nonnull(height);
    g_assert_nonnull(apply);
    guint previous_writes = p.writes;
    gtk_drop_down_set_selected(GTK_DROP_DOWN(presets), 4);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(width)),
                    ==, 4000);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(height)),
                    ==, 3111);
    g_assert_cmpuint(p.writes, ==, previous_writes);
    clbb_settings_refresh(s);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(height)),
                    ==, 3111);
    g_signal_emit_by_name(apply, "clicked");
    g_assert_cmpuint(p.writes, ==, previous_writes + 1);
    g_assert_cmpstr(read_value(&p, "display.resolution"), ==, "4000x3111");
    g_assert_false(gtk_widget_get_sensitive(apply));
    gtk_drop_down_set_selected(GTK_DROP_DOWN(presets), 2);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(height), 1081);
    clbb_settings_refresh(s);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(width)),
                    ==, 1920);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(height)),
                    ==, 1081);
    g_assert_cmpuint(p.writes, ==, previous_writes + 1);
    g_signal_emit_by_name(apply, "clicked");
    g_assert_cmpstr(read_value(&p, "display.resolution"), ==, "1920x1081");
    g_assert_cmpuint(p.writes, ==, previous_writes + 2);
    g_assert(!write_value(&p, "display.resolution", "8193x720"));
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(width), 9000);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(width)),
                    ==, 8192);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(width), 1);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(width)),
                    ==, 256);
    gtk_drop_down_set_selected(GTK_DROP_DOWN(presets), 0);
    gtk_editable_set_text(GTK_EDITABLE(width), "3333");
    clbb_settings_refresh(s);
    g_assert_cmpstr(gtk_editable_get_text(GTK_EDITABLE(width)), ==, "3333");
    g_assert_cmpuint(p.writes, ==, previous_writes + 2);
    g_signal_emit_by_name(apply, "clicked");
    g_assert_cmpstr(read_value(&p, "display.resolution"), ==, "3333x1081");
    g_assert_cmpuint(p.writes, ==, previous_writes + 3);
    GtkWidget *fps_preset = find_control(window, "display.fps_preset");
    GtkWidget *fps_spin = find_control(window, "display.fps_limit");
    GtkWidget *vsync = find_control(window, "display.vsync");
    GtkWidget *pacing_apply = find_button(window, "Применить FPS");
    g_assert_nonnull(fps_preset);
    g_assert_nonnull(fps_spin);
    g_assert_nonnull(vsync);
    g_assert_nonnull(pacing_apply);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(fps_spin)),
                    ==, 60);
    g_assert_true(gtk_switch_get_active(GTK_SWITCH(vsync)));
    g_assert_false(gtk_widget_get_sensitive(pacing_apply));
    guint pacing_writes = p.writes;
    gtk_drop_down_set_selected(GTK_DROP_DOWN(fps_preset), 7);
    gtk_switch_set_active(GTK_SWITCH(vsync), FALSE);
    clbb_settings_refresh(s);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(fps_spin)),
                    ==, 240);
    g_assert_false(gtk_switch_get_active(GTK_SWITCH(vsync)));
    g_assert_cmpuint(p.writes, ==, pacing_writes);
    g_signal_emit_by_name(pacing_apply, "clicked");
    g_assert_cmpuint(p.writes, ==, pacing_writes + 1);
    g_assert_cmpstr(read_value(&p, "display.frame_pacing"), ==,
                    "limit=240;vsync=0");
    g_assert_false(gtk_widget_get_sensitive(pacing_apply));
    int limits[] = {30, 60, 90, 120, 144, 165, 180, 240};
    for (guint n = 0; n < G_N_ELEMENTS(limits); ++n) {
      gtk_drop_down_set_selected(GTK_DROP_DOWN(fps_preset), n);
      g_assert_cmpint(
          gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(fps_spin)), ==,
          limits[n]);
    }
    g_assert_cmpuint(p.writes, ==, pacing_writes + 1);
    gtk_editable_set_text(GTK_EDITABLE(fps_spin), "137");
    gtk_switch_set_active(GTK_SWITCH(vsync), TRUE);
    gtk_switch_set_active(GTK_SWITCH(vsync), FALSE);
    clbb_settings_refresh(s);
    g_assert_cmpstr(gtk_editable_get_text(GTK_EDITABLE(fps_spin)), ==, "137");
    gtk_switch_set_active(GTK_SWITCH(vsync), TRUE);
    g_signal_emit_by_name(pacing_apply, "clicked");
    g_assert_cmpstr(read_value(&p, "display.frame_pacing"), ==,
                    "limit=137;vsync=1");
    g_assert_cmpuint(p.writes, ==, pacing_writes + 2);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(fps_spin), 999);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(fps_spin)),
                    ==, 240);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(fps_spin), 1);
    g_assert_cmpint(gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(fps_spin)),
                    ==, 30);
    g_assert_cmpuint(p.writes, ==, pacing_writes + 2);
    gtk_window_present(preview);
    clbb_settings_show(s);
    g_assert(clbb_settings_is_visible(s));
    g_assert(gtk_widget_get_root(window) == GTK_ROOT(preview));
    g_assert_nonnull(find_button(window, "Вернуться в игру"));
    /* Real layout on the isolated server must fit the selected monitor,
     * including narrow displays where the old fixed minimum overflowed. */
    for (guint n = 0; n < 100; ++n) {
      while (g_main_context_iteration(NULL, FALSE)) {
      }
      if (gtk_widget_get_width(window) > 0 && gtk_widget_get_height(window) > 0)
        break;
      g_usleep(1000);
    }
    GdkSurface *surface = gtk_native_get_surface(GTK_NATIVE(preview));
    monitor = gdk_display_get_monitor_at_surface(
        gtk_widget_get_display(window), surface);
    GdkRectangle geometry;
    gdk_monitor_get_geometry(monitor, &geometry);
    g_assert_cmpint(gtk_widget_get_width(window), <=, geometry.width - 32);
    g_assert_cmpint(gtk_widget_get_height(window), <=, geometry.height - 64);
    g_print("CLBB_SETTINGS_GEOMETRY_OK window=%dx%d monitor=%dx%d\n",
            gtk_widget_get_width(window), gtk_widget_get_height(window),
            geometry.width, geometry.height);
    clbb_settings_hide(s);
    g_assert(!clbb_settings_is_visible(s));
    g_assert_cmpuint(p.visible, ==, 1);
    put(&p, "hero.id", "133");put(&p, "hero.linked_controls", "1");
    put(&p, "hero.skill0_type", "1");
    clbb_settings_refresh(s);
    g_assert_nonnull(find_control(window, "hero.bindings.combo:0.key"));
    g_assert_nonnull(find_control(window, "hero.bindings.combo:1.key"));
    g_assert_null(find_control(window, "hero.bindings.combo:0.cast_mode"));
    g_assert_null(find_control(window, "hero.bindings.combo:1.cast_mode"));
    g_assert_nonnull(find_control(window, "hero.bindings.skill:0.cast_mode"));
    g_assert_nonnull(find_control(window, "hero.bindings.skill:1.cast_mode"));
    g_assert_null(find_control(window, "hero.bindings.skill:2.key"));
    g_assert(write_value(&p, "hero.bindings.combo:0.key", "88"));
    g_assert_cmpstr(read_value(&p, "hero.bindings.combo:0.key"), ==, "88");
    put(&p, "hero.id", "25");put(&p, "hero.linked_controls", "0");
    clbb_settings_refresh(s);
    g_assert_null(find_control(window, "hero.bindings.combo:0.key"));
    g_assert_nonnull(find_control(window, "hero.bindings.skill:2.key"));
    g_print("CLBB_LINKED_BINDINGS_UI_OK personal keys, capability refresh, no combo aim modes\n");
    put(&p,"hero.id","126");
    put(&p,"hero.name","Су Ё");
    put(&p,"preview.controls","1\t126\nskill:0\tskill:0\tskill:0\t12610\t12610\t2\nskill:0\tskill:0:12611\tskill:0:12611\t12610\t12611\t2\nskill:1\tskill:1\tskill:1\t12630\t12630\t2\nskill:1\tskill:1:12632\tskill:1:12632\t12630\t12632\t2\nskill:2\tskill:2\tskill:2\t12650\t12650\t2\nskill:2\tskill:2:12652\tskill:2:12652\t12650\t12652\t2\n");
    clbb_settings_refresh(s);
    for(int i=0;i<3;i++) {
      char *key=g_strdup_printf("hero.bindings.skill:%d.cast_mode",i);
      g_assert_null(find_control(window,key));g_free(key);
      key=g_strdup_printf("common.bindings.skill:%d.cast_mode",i);
      g_assert_null(find_control(window,key));g_free(key);
      key=g_strdup_printf("hero.bindings.skill:%d.key",i);
      g_assert_nonnull(find_control(window,key));g_free(key);
      key=g_strdup_printf("hero.bindings.skill:%d.stop_movement",i);
      g_assert_nonnull(find_control(window,key));g_free(key);
    }
    g_hash_table_remove(p.values,"preview.controls");put(&p,"hero.id","25");clbb_settings_refresh(s);
    g_assert_nonnull(find_control(window,"hero.bindings.skill:0.cast_mode"));
    g_print("CLBB_SUYOU_SINGLE_HOLD_SKILL_UI_OK three fixed release modes, shared stop/key settings\n");
    g_print("CLBB_SETTINGS_SELFTEST_OK key normalization, reserved LMB, hotkey "
            "conflicts, refresh, hero switch, overrides, stop movement, "
            "atomic display apply, preserved draft, atomic FPS VSync, "
            "visibility\n");
  } else {
    const char *tabs[] = {"main", "controls", "sound", "hotkeys"};
    GtkWidget *nav =
        find_type(GTK_WIDGET(clbb_settings_panel(s)), GTK_TYPE_LIST_BOX);
    for (guint n = 0; n < G_N_ELEMENTS(tabs); ++n)
      if (nav && !strcmp(tab, tabs[n]))
        gtk_list_box_select_row(
            GTK_LIST_BOX(nav),
            gtk_list_box_get_row_at_index(GTK_LIST_BOX(nav), n));
    if (expand_first) {
      GtkWidget *row=find_control(clbb_settings_panel(s),"hero.bindings.skill:0.key");
      GtkWidget *reveal=gtk_widget_get_ancestor(row,GTK_TYPE_REVEALER);
      gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(gtk_widget_get_first_child(gtk_widget_get_parent(reveal))),TRUE);
    }
    g_signal_connect(clbb_settings_panel(s), "notify::visible",
                     G_CALLBACK(visible_changed), &p);
    gtk_window_present(preview);
    clbb_settings_show(s);
    if (timeout)
      g_timeout_add_seconds(timeout, quit_later, p.loop);
    g_main_loop_run(p.loop);
  }
  clbb_settings_destroy(s);
  gtk_window_destroy(preview);
  g_main_loop_unref(p.loop);
  g_hash_table_unref(p.values);
  return 0;
}
