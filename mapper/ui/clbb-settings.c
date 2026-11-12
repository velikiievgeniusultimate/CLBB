#include "clbb-settings.h"
#include <stdlib.h>
#include <string.h>

#define KEY_F1 0x01000030
#define KEY_MOUSE 0x02000000

typedef enum {
  CONTROL_BIND,
  CONTROL_SWITCH,
  CONTROL_NUMBER,
  CONTROL_MODE,
  CONTROL_VOLUME
} ControlKind;
typedef struct {
  struct ClbbSettings *owner;
  char *key;
  GtkWidget *widget;
  GtkWidget *row;
  ControlKind kind;
  int fallback;
  gboolean dependent;
  gboolean battle_spell_mode;
  gboolean plain_spell_mode;
  char *override_key;
  GtkWidget *override_widget;
} Control;

struct ClbbSettings {
  GtkWidget *debug_status;
  GtkWidget *audio_input, *audio_output, *audio_status;
  GPtrArray *audio_input_ids, *audio_output_ids;
  char *audio_inventory[2];
  GtkWidget *panel;
  GWeakRef previous_focus;
  ClbbSettingsCallbacks callbacks;
  GPtrArray *controls;
  GtkWidget *hero_box;
  GtkWidget *common_box;
  GtkWidget *brand;
  GtkWidget *message;
  GtkWidget *capture_hint;
  GtkWidget *movement_distance_row;
  GtkWidget *display_presets;
  GtkWidget *display_width;
  GtkWidget *display_height;
  GtkWidget *display_apply;
  GtkWidget *display_current;
  int display_current_width;
  int display_current_height;
  gboolean display_dirty;
  GtkWidget *fps_preset;
  GtkWidget *fps_spin;
  GtkWidget *vsync_switch;
  GtkWidget *pacing_apply;
  GtkWidget *pacing_current;
  int fps_current;
  gboolean vsync_current;
  gboolean pacing_dirty;
  GtkWidget *update_version;
  GtkWidget *update_status;
  GtkWidget *update_progress;
  GtkWidget *update_check;
  GtkWidget *update_install;
  GtkStack *stack;
  Control *capture;
  int hero;
  int native_skill_types[4];
  int native_selection_kinds[4];
  gboolean linked_controls;
  gboolean refreshing;
};

static const char css[] =
    ".clbb-settings { background: #141216; color: #f2edf0; font-family: "
    "Cantarell, sans-serif; font-size: 14px; }"
    ".clbb-shell { background: #141216; } .clbb-header { padding: 20px 26px; "
    "border-bottom: 1px solid #392832; }"
    ".clbb-brand { font-size: 19px; font-weight: 800; letter-spacing: 2px; } "
    ".clbb-caption { color: #bd8b9b; font-size: 11px; letter-spacing: 2px; }"
    ".clbb-sidebar { background: #19151b; border-right: 1px solid #392832; "
    "padding: 18px 12px; }"
    ".clbb-sidebar row { border-radius: 5px; margin-bottom: 6px; padding: 0; }"
    ".clbb-sidebar row:selected { background: #3b2230; color: #fff8fa; } "
    ".clbb-sidebar row:hover { background: #2a2029; }"
    ".clbb-nav-label { padding: 15px 17px; font-weight: 600; color: #cbbac8; } "
    ".clbb-sidebar row:selected .clbb-nav-label { color: #fff8fa; } "
    ".clbb-content { padding: 24px 28px; }"
    ".clbb-title { font-size: 24px; font-weight: 750; margin-bottom: 5px; } "
    ".clbb-muted { color: #b4a8b3; font-size: 13px; }"
    ".clbb-section { background: #211c23; border: 1px solid #49313e; "
    "border-radius: 7px; margin-top: 17px; padding: 18px; }"
    ".clbb-section-title { font-weight: 750; font-size: 16px; }"
    "button.clbb-section-heading { background: transparent; background-image: none; border: none; box-shadow: none; padding: 4px 0; color: #f2edf0; }"
    "button.clbb-section-heading:hover { color: #ff668d; }"
    ".clbb-section-content { margin-top: 12px; }"
    ".clbb-row { padding: 7px 0; border-bottom: 1px solid alpha(#927a8c, "
    "0.12); } .clbb-row:last-child { border-bottom: none; }"
    ".clbb-label { font-weight: 550; } .clbb-bind { min-width: 92px; "
    "min-height: 31px; border: 1px solid #614052; background: #30242e; color: "
    "#fff2f6; border-radius: 5px; font-weight: 700; }"
    ".clbb-bind:disabled { opacity: 0.45; } .clbb-bind:hover { background: "
    "#4a2b3a; border-color: #d45075; } "
    ".clbb-bind.capture { background: #b7365c; border-color: #ff668d; } .clbb-recording { min-width: 142px; min-height: 30px; background: #30242e; color: #e7dce4; border: 1px solid #614052; border-radius: 5px; font-weight: 700; } .clbb-recording:checked { background: #a9234b; color: white; border-color: #ff668d; }"
    "button.clbb-close { background: #2a2029; color: #eee6ec; border: 1px "
    "solid #503344; border-radius: 5px; min-width: 30px; min-height: 28px; }"
    "button.clbb-secondary { background: #2a2029; background-image: none; "
    "color: #d8c9d3; border: 1px solid #503344; border-radius: 5px; "
    "box-shadow: none; text-shadow: none; padding: 6px 12px; min-height: 24px; }"
    "button.clbb-secondary:hover { background: #382632; border-color: #99506b; color: #fff2f6; }"
    "button.clbb-secondary:active { background: #4a2939; border-color: #ed5279; }"
    "button.clbb-secondary:focus-visible { outline: 2px solid #ed5279; outline-offset: 2px; }"
    ".clbb-settings switch:checked { background: #de4a70; border-color: "
    "#f06788; } .clbb-settings dropdown, .clbb-settings spinbutton "
    "{ background: #2a222c; border: 1px solid #513646; color: #f0e8ee; "
    "border-radius: 5px; }"
    ".clbb-settings dropdown button { background: transparent; border: "
    "none; color: #f0e8ee; } .clbb-settings dropdown label, "
    ".clbb-settings dropdown arrow { color: #f0e8ee; } "
    ".clbb-settings spinbutton text, .clbb-settings scale value { "
    "color: #f0e8ee; } .clbb-settings popover contents { background: "
    "#29212b; color: #eee7ed; border: 1px solid #634357; }"
    ".clbb-settings dropdown popover listview { background: #211b23; "
    "color: #f2edf0; padding: 4px; }"
    ".clbb-settings dropdown popover row { background: #211b23; "
    "border-radius: 4px; padding: 3px 6px; }"
    ".clbb-settings dropdown popover row:hover { background: #382735; }"
    ".clbb-settings dropdown popover row:selected { background: #622a40; }"
    ".clbb-settings dropdown popover row label { color: #f2edf0; }"
    ".clbb-settings dropdown popover row:selected label { color: #fff5f8; }"
    ".clbb-settings dropdown popover row image { color: #ff91ac; }"
    ".clbb-settings scale trough { background: #443341; min-height: 5px; "
    "} .clbb-settings scale highlight { background: #ed5279; } "
    ".clbb-settings scale slider { background: #f0e4ec; border: 1px "
    "solid #e65b7e; }"
    ".clbb-footer { border-top: 1px solid #392832; padding: 10px 24px; color: "
    "#baacb6; font-size: 12px; }"
    ".clbb-progress progress { background: #d84b70; } .clbb-progress trough { background: #232735; min-height: 8px; } "
    ".clbb-accent { color: #ff6b91; } .clbb-info { color: #9dbad3; } "
    ".clbb-override { font-size: 11px; color: #c5b4c0; }";

static char *read_text(ClbbSettings *s, const char *key, const char *fallback) {
  const char *v =
      s->callbacks.read ? s->callbacks.read(s->callbacks.user, key) : NULL;
  return g_strdup(v && *v ? v : fallback);
}
static int read_int(ClbbSettings *s, const char *key, int fallback) {
  char *v = read_text(s, key, "");
  char *end = NULL;
  gint64 n = g_ascii_strtoll(v, &end, 0);
  int answer =
      *v && end && !*end && n >= G_MININT && n <= G_MAXINT ? (int)n : fallback;
  g_free(v);
  return answer;
}
static gboolean read_bool(ClbbSettings *s, const char *key, gboolean fallback) {
  char *v = read_text(s, key, fallback ? "true" : "false");
  gboolean result = !g_strcmp0(v, "true") || !g_strcmp0(v, "1");
  g_free(v);
  return result;
}
static gboolean write_value(ClbbSettings *s, const char *key,
                            const char *value) {
  if (s->refreshing)
    return TRUE;
  gboolean ok =
      s->callbacks.write && s->callbacks.write(s->callbacks.user, key, value);
  gtk_label_set_text(GTK_LABEL(s->message),
                     ok ? "Изменения сохранены"
                        : "Не удалось сохранить: проверьте назначение кнопки");
  if (!ok)
    clbb_settings_refresh(s);
  return ok;
}
static void audio_selected(GObject *widget, GParamSpec *pspec, gpointer user) {
  (void)pspec;ClbbSettings *s=user;if(s->refreshing)return;
  gboolean input=GTK_WIDGET(widget)==s->audio_input;
  GPtrArray *ids=input?s->audio_input_ids:s->audio_output_ids;
  guint selected=gtk_drop_down_get_selected(GTK_DROP_DOWN(widget));
  if(ids && selected<ids->len)write_value(s,input?"audio.input_device":"audio.output_device",g_ptr_array_index(ids,selected));
}
static void audio_refresh_clicked(GtkButton *button,gpointer user) {
  (void)button;ClbbSettings *s=user;write_value(s,"audio.refresh","1");
}
static void audio_refresh(ClbbSettings *s) {
  for(int i=0;i<2;i++) {
    GtkWidget *widget=i?s->audio_output:s->audio_input;
    if(!widget)continue;
    char *rows=read_text(s,i?"audio.output_devices":"audio.input_devices","");
    char *chosen=read_text(s,i?"audio.output_device":"audio.input_device","default");
    char *inventory=g_strconcat(rows,"|",chosen,NULL);
    if(g_strcmp0(inventory,s->audio_inventory[i])) {
      g_free(s->audio_inventory[i]);s->audio_inventory[i]=inventory;inventory=NULL;
      GPtrArray **ids=i?&s->audio_output_ids:&s->audio_input_ids;
      if(*ids)g_ptr_array_unref(*ids);*ids=g_ptr_array_new_with_free_func(g_free);
      GtkStringList *model=gtk_string_list_new(NULL);
      gtk_string_list_append(model,"Системное устройство по умолчанию");g_ptr_array_add(*ids,g_strdup("default"));
      char **lines=g_strsplit(rows,"\n",-1);guint selected=0;gboolean found=!g_strcmp0(chosen,"default");
      for(int j=0;lines[j];j++) {char *tab=strchr(lines[j],'\t');if(!tab)continue;*tab=0;
        if(!*lines[j]||!g_strcmp0(lines[j],"default"))continue;
        gtk_string_list_append(model,tab+1);g_ptr_array_add(*ids,g_strdup(lines[j]));
        if(!g_strcmp0(chosen,lines[j])){selected=(*ids)->len-1;found=TRUE;}}
      if(!found){char *label=g_strdup_printf("Недоступно: %s",chosen);gtk_string_list_append(model,label);g_free(label);g_ptr_array_add(*ids,g_strdup(chosen));selected=(*ids)->len-1;}
      gtk_drop_down_set_model(GTK_DROP_DOWN(widget),G_LIST_MODEL(model));gtk_drop_down_set_selected(GTK_DROP_DOWN(widget),selected);g_object_unref(model);g_strfreev(lines);
    }
    g_free(inventory);g_free(rows);g_free(chosen);
  }
  if(s->audio_status){char *status=read_text(s,"audio.status","Поиск аудиоустройств…");gtk_label_set_text(GTK_LABEL(s->audio_status),status);g_free(status);}
}
static gboolean write_int(ClbbSettings *s, const char *key, int value) {
  char v[32];
  g_snprintf(v, sizeof(v), "%d", value);
  return write_value(s, key, v);
}

gint clbb_settings_chord(gint code, GdkModifierType state) {
  return code ? code | (state & GDK_CONTROL_MASK ? 0x04000000 : 0) |
      (state & GDK_SHIFT_MASK ? 0x02000000 : 0) |
      (state & GDK_ALT_MASK ? 0x08000000 : 0) : 0;
}
static char *key_name(int code) {
  if (!code)
    return g_strdup("Не задана");
  if (code >= KEY_F1 && code <= KEY_F1 + 34)
    return g_strdup_printf("F%d", code - KEY_F1 + 1);
  if (code & KEY_MOUSE) {
    switch (code & ~KEY_MOUSE) {
    case 1:
      return g_strdup("ЛКМ");
    case 2:
      return g_strdup("ПКМ");
    case 4:
      return g_strdup("СКМ");
    case 8:
      return g_strdup("Mouse 4");
    case 16:
      return g_strdup("Mouse 5");
    default:
      return g_strdup_printf("Mouse %d", code & ~KEY_MOUSE);
    }
  }
  switch (code) {
  case 32:
    return g_strdup("Пробел");
  case 0x01000000:
    return g_strdup("Esc");
  case 0x01000001:
    return g_strdup("Tab");
  case 0x01000003:
    return g_strdup("Backspace");
  case 0x01000004:
  case 0x01000005:
    return g_strdup("Enter");
  case 0x01000006:
    return g_strdup("Insert");
  case 0x01000007:
    return g_strdup("Delete");
  case 0x01000010:
    return g_strdup("Home");
  case 0x01000011:
    return g_strdup("End");
  case 0x01000012:
    return g_strdup("←");
  case 0x01000013:
    return g_strdup("↑");
  case 0x01000014:
    return g_strdup("→");
  case 0x01000015:
    return g_strdup("↓");
  case 0x01000016:
    return g_strdup("Page Up");
  case 0x01000017:
    return g_strdup("Page Down");
  default:
    if (code > 0 && code < 0x110000) {
      char b[8] = {0};
      g_unichar_to_utf8(code, b);
      return g_strdup(b);
    }
    return g_strdup_printf("0x%X", code);
  }
}

gint clbb_settings_keycode(guint keyval) {
  if (keyval >= GDK_KEY_F1 && keyval <= GDK_KEY_F35)
    return KEY_F1 + (int)(keyval - GDK_KEY_F1);
  switch (keyval) {
  case GDK_KEY_Escape:
    return 0x01000000;
  case GDK_KEY_Tab:
  case GDK_KEY_ISO_Left_Tab:
    return 0x01000001;
  case GDK_KEY_BackSpace:
    return 0x01000003;
  case GDK_KEY_Return:
    return 0x01000004;
  case GDK_KEY_KP_Enter:
    return 0x01000005;
  case GDK_KEY_Insert:
    return 0x01000006;
  case GDK_KEY_Delete:
    return 0x01000007;
  case GDK_KEY_Home:
    return 0x01000010;
  case GDK_KEY_End:
    return 0x01000011;
  case GDK_KEY_Left:
    return 0x01000012;
  case GDK_KEY_Up:
    return 0x01000013;
  case GDK_KEY_Right:
    return 0x01000014;
  case GDK_KEY_Down:
    return 0x01000015;
  case GDK_KEY_Page_Up:
    return 0x01000016;
  case GDK_KEY_Page_Down:
    return 0x01000017;
  case GDK_KEY_Shift_L:
  case GDK_KEY_Shift_R:
  case GDK_KEY_Control_L:
  case GDK_KEY_Control_R:
  case GDK_KEY_Alt_L:
  case GDK_KEY_Alt_R:
  case GDK_KEY_Meta_L:
  case GDK_KEY_Meta_R:
  case GDK_KEY_Super_L:
  case GDK_KEY_Super_R:
    return 0;
  default: {
    gunichar u = gdk_keyval_to_unicode(keyval);
    return u >= 32 && u < 0x110000 ? (gint)g_unichar_toupper(u) : 0;
  }
  }
}
static GtkWidget *label(const char *text, const char *style) {
  GtkWidget *w = gtk_label_new(text);
  gtk_label_set_xalign(GTK_LABEL(w), 0);
  if (style)
    gtk_widget_add_css_class(w, style);
  return w;
}
static GtkWidget *wrap_label(const char *text, const char *style) {
  GtkWidget *w = label(text, style);
  gtk_label_set_wrap(GTK_LABEL(w), TRUE);
  gtk_label_set_wrap_mode(GTK_LABEL(w), PANGO_WRAP_WORD_CHAR);
  return w;
}
static void stop_capture(ClbbSettings *s);
static void section_toggled(GtkToggleButton *button, gpointer data) {
  gboolean open = gtk_toggle_button_get_active(button);
  gtk_revealer_set_reveal_child(GTK_REVEALER(data), open);
  ClbbSettings *s = g_object_get_data(G_OBJECT(data), "clbb-settings-owner");
  if (!open && s && s->capture && gtk_widget_is_ancestor(s->capture->row, GTK_WIDGET(data)))
    stop_capture(s);
  GtkWidget *arrow = g_object_get_data(G_OBJECT(button), "clbb-chevron");
  gtk_image_set_from_icon_name(GTK_IMAGE(arrow), open ? "pan-down-symbolic" : "pan-end-symbolic");
}
static gboolean section_is_open(GtkWidget *container) {
  GtkWidget *card = gtk_widget_get_first_child(container);
  return card && gtk_toggle_button_get_active(GTK_TOGGLE_BUTTON(gtk_widget_get_first_child(card)));
}
static void section_set_open(GtkWidget *container, gboolean open) {
  GtkWidget *card = gtk_widget_get_first_child(container);
  if (card) gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(gtk_widget_get_first_child(card)), open);
}
static GtkWidget *section(GtkWidget *box, const char *title) {
  GtkWidget *card = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
  gtk_widget_add_css_class(card, "clbb-section");
  GtkWidget *header = gtk_toggle_button_new();
  gtk_widget_add_css_class(header, "clbb-section-heading");
  GtkWidget *heading = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 12);
  GtkWidget *text = label(title ? title : "Настройки", "clbb-section-title");
  gtk_widget_set_hexpand(text, TRUE);
  gtk_box_append(GTK_BOX(heading), text);
  GtkWidget *arrow = gtk_image_new_from_icon_name("pan-end-symbolic");
  gtk_box_append(GTK_BOX(heading), arrow);
  gtk_button_set_child(GTK_BUTTON(header), heading);
  gtk_box_append(GTK_BOX(card), header);
  // Revealer retains widget ancestry while closed: hidden bind controls remain
  // reachable for refresh and are removed safely when the hero profile changes.
  GtkWidget *revealer = gtk_revealer_new();
  gtk_revealer_set_reveal_child(GTK_REVEALER(revealer), FALSE);
  gtk_revealer_set_transition_duration(GTK_REVEALER(revealer), 120);
  GtkWidget *content = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
  gtk_widget_add_css_class(content, "clbb-section-content");
  gtk_revealer_set_child(GTK_REVEALER(revealer), content);
  gtk_box_append(GTK_BOX(card), revealer);
  g_object_set_data(G_OBJECT(header), "clbb-chevron", arrow);
  g_signal_connect(header, "toggled", G_CALLBACK(section_toggled), revealer);
  gtk_box_append(GTK_BOX(box), card);
  return content;
}
static GtkWidget *page(ClbbSettings *s, const char *name, const char *title,
                       const char *subtitle) {
  GtkWidget *scroll = gtk_scrolled_window_new();
  gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroll),
                                 GTK_POLICY_AUTOMATIC, GTK_POLICY_AUTOMATIC);
  GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
  gtk_widget_add_css_class(box, "clbb-content");
  gtk_box_append(GTK_BOX(box), label(title, "clbb-title"));
  if (subtitle)
    gtk_box_append(GTK_BOX(box), wrap_label(subtitle, "clbb-muted"));
  gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroll), box);
  gtk_stack_add_named(s->stack, scroll, name);
  return box;
}
static void control_free(void *v) {
  Control *c = v;
  g_free(c->key);
  g_free(c->override_key);
  g_free(c);
}
static Control *new_control(ClbbSettings *s, GtkWidget *box, const char *title,
                            const char *key, ControlKind kind, int fallback) {
  Control *c = g_new0(Control, 1);
  c->owner = s;
  GtkWidget *group = gtk_widget_get_ancestor(box, GTK_TYPE_REVEALER);
  if (group) g_object_set_data(G_OBJECT(group), "clbb-settings-owner", s);
  c->key = g_strdup(key);
  c->kind = kind;
  c->fallback = fallback;
  c->row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 16);
  g_object_set_data_full(G_OBJECT(c->row), "clbb-settings-key", g_strdup(key),
                         g_free);
  gtk_widget_add_css_class(c->row, "clbb-row");
  GtkWidget *text = wrap_label(title, "clbb-label");
  gtk_widget_set_hexpand(text, TRUE);
  gtk_box_append(GTK_BOX(c->row), text);
  gtk_box_append(GTK_BOX(box), c->row);
  g_ptr_array_add(s->controls, c);
  return c;
}
static void stop_capture(ClbbSettings *s) {
  if (s->capture) {
    gtk_widget_remove_css_class(s->capture->widget, "capture");
    s->capture = NULL;
  }
  gtk_label_set_text(
      GTK_LABEL(s->capture_hint),
      "ЛКМ всегда управляет игрой · настройки сохраняются автоматически");
}
static void begin_capture(GtkButton *button, gpointer data) {
  Control *c = data;
  ClbbSettings *s = c->owner;
  stop_capture(s);
  s->capture = c;
  gtk_widget_add_css_class(GTK_WIDGET(button), "capture");
  gtk_button_set_label(button, "Нажмите…");
  gtk_label_set_text(
      GTK_LABEL(s->capture_hint),
      "Нажмите клавишу или ПКМ · Esc — отмена · Backspace — очистить");
}
static void toggled(GObject *object, GParamSpec *param, gpointer data) {
  (void)param;
  Control *c = data;
  write_value(c->owner, c->key,
              gtk_switch_get_active(GTK_SWITCH(object)) ? "true" : "false");
}
static void recording_toggled(GtkToggleButton *button, gpointer data) {
  Control *c = data;
  gboolean active = gtk_toggle_button_get_active(button);
  gtk_button_set_label(GTK_BUTTON(button), active ? "ВКЛ · запись" : "ВЫКЛ · запись");
  write_value(c->owner, c->key, active ? "true" : "false");
}
static void changed_number(GtkSpinButton *spin, gpointer data) {
  Control *c = data;
  write_int(c->owner, c->key, gtk_spin_button_get_value_as_int(spin));
}
static void changed_mode(GObject *object, GParamSpec *param, gpointer data) {
  (void)param;
  Control *c = data;
  int selected = (int)gtk_drop_down_get_selected(GTK_DROP_DOWN(object));
  static const int spell_modes[] = {2, 0, 1, 3};
  if (c->battle_spell_mode && !c->plain_spell_mode && selected >= 0 && selected < 4)
    selected = spell_modes[selected];
  if (write_int(c->owner, c->key, selected) &&
      !strcmp(c->key, "movement.click_mode"))
    gtk_widget_set_visible(c->owner->movement_distance_row, selected == 0);
}
static void changed_volume(GtkRange *range, gpointer data) {
  Control *c = data;
  write_int(c->owner, c->key, (int)(gtk_range_get_value(range) + 0.5));
}
static Control *bind_control(ClbbSettings *s, GtkWidget *box, const char *title,
                             const char *key, int fallback) {
  Control *c = new_control(s, box, title, key, CONTROL_BIND, fallback);
  c->widget = gtk_button_new();
  gtk_widget_add_css_class(c->widget, "clbb-bind");
  g_signal_connect(c->widget, "clicked", G_CALLBACK(begin_capture), c);
  gtk_box_append(GTK_BOX(c->row), c->widget);
  return c;
}
static Control *switch_control(ClbbSettings *s, GtkWidget *box,
                               const char *title, const char *key,
                               int fallback) {
  Control *c = new_control(s, box, title, key, CONTROL_SWITCH, fallback);
  if (!strcmp(key, "debug.recording")) {
    c->widget = gtk_toggle_button_new_with_label("ВЫКЛ · запись");
    gtk_widget_add_css_class(c->widget, "clbb-recording");
    g_signal_connect(c->widget, "toggled", G_CALLBACK(recording_toggled), c);
  } else {
    c->widget = gtk_switch_new();
    g_signal_connect(c->widget, "notify::active", G_CALLBACK(toggled), c);
  }
  gtk_widget_set_valign(c->widget, GTK_ALIGN_CENTER);
  gtk_box_append(GTK_BOX(c->row), c->widget);
  return c;
}
static void mode_control(ClbbSettings *s, GtkWidget *box, const char *title,
                         const char *key, int fallback, gboolean movement) {
  Control *c = new_control(s, box, title, key, CONTROL_MODE, fallback);
  const char *const casts[] = {"При отпускании", "Сразу при нажатии", NULL};
  const char *const moves[] = {"По прямой", "С обходом препятствий", NULL};
  const char *const spells[] = {"Моментальное прожатие", "Обычный навык",
                                "Моментальный с наведением",
                                "Прожатие с удержанием", NULL};
  c->battle_spell_mode = g_str_has_suffix(key, ".bindings.extra:2.cast_mode");
  c->widget = gtk_drop_down_new_from_strings(
      c->battle_spell_mode ? spells : movement ? moves : casts);
  g_signal_connect(c->widget, "notify::selected", G_CALLBACK(changed_mode), c);
  gtk_box_append(GTK_BOX(c->row), c->widget);
}
static void override_changed(GObject *object, GParamSpec *param,
                             gpointer data) {
  (void)param;
  Control *c = data;
  if (write_value(c->owner, c->override_key,
                  gtk_switch_get_active(GTK_SWITCH(object)) ? "true" : "false"))
    clbb_settings_refresh(c->owner);
}
static void add_binding(ClbbSettings *s, GtkWidget *box, const char *title,
                        const char *prefix, const char *slot, int key, int mode,
                        gboolean tap, gboolean overrides) {
  char *k = g_strdup_printf("%s.bindings.%s.key", prefix, slot);
  char *selection_title = NULL;
  if (!strcmp(prefix, "hero") && g_str_has_prefix(slot, "skill:") &&
      slot[6] >= '0' && slot[6] <= '3' && slot[7] == '\0') {
    char type_key[] = "hero.skill0_type";
    type_key[10] = slot[6];
    if (read_int(s, type_key, 0) == 4) {
      char kind_key[] = "hero.skill0_selection_kind";
      kind_key[10] = slot[6];
      int kind = read_int(s, kind_key, 0);
      selection_title = g_strdup_printf("%s · %s", title,
          kind == 1 ? "Выбор варианта" : kind == 2 ? "Выбор цели" : "Выбор");
    }
  }
  Control *c = bind_control(s, box, selection_title ? selection_title : title, k, key);
  g_free(selection_title);
  g_free(k);
  if (overrides) {
    c->override_key = g_strdup_printf("hero.bindings.%s.override", slot);
    GtkWidget *outer = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 7);
    gtk_box_append(GTK_BOX(outer), label("Своя", "clbb-override"));
    c->override_widget = gtk_switch_new();
    gtk_widget_set_valign(c->override_widget, GTK_ALIGN_CENTER);
    g_signal_connect(c->override_widget, "notify::active",
                     G_CALLBACK(override_changed), c);
    gtk_box_append(GTK_BOX(outer), c->override_widget);
    gtk_box_append(GTK_BOX(c->row), outer);
  }
  if (!tap) {
    k = g_strdup_printf("%s.bindings.%s.cast_mode", prefix, slot);
    mode_control(s, box, "Реализация навыка", k, mode, FALSE);
    g_free(k);
    if (overrides) {
      Control *m = g_ptr_array_index(s->controls, s->controls->len - 1);
      m->override_key = g_strdup(c->override_key);
      m->dependent = TRUE;
    }
    if (!strcmp(slot, "extra:2") ||
        (!strcmp(prefix, "hero") && g_str_has_prefix(slot, "skill:"))) {
      k = g_strdup_printf("%s.bindings.%s.stop_movement", prefix, slot);
      Control *stop =
          switch_control(s, box, "Останавливать движение при применении", k, 0);
      g_free(k);
      if (overrides) {
        stop->override_key = g_strdup(c->override_key);
        stop->dependent = TRUE;
      }
    }
  }
}
static void clear_dynamic(ClbbSettings *s, GtkWidget *box) {
  for (gint i = (gint)s->controls->len - 1; i >= 0; --i) {
    Control *c = g_ptr_array_index(s->controls, i);
    if (gtk_widget_is_ancestor(c->row, box)) {
      if (s->capture == c)
        stop_capture(s);
      g_ptr_array_remove_index(s->controls, i);
    }
  }
  GtkWidget *w;
  while ((w = gtk_widget_get_first_child(box)))
    gtk_box_remove(GTK_BOX(box), w);
}
static void build_hero(ClbbSettings *s, int hero) {
  gboolean keep_personal = hero == s->hero && section_is_open(s->hero_box);
  gboolean keep_overrides = hero == s->hero && section_is_open(s->common_box);
  clear_dynamic(s, s->hero_box);
  clear_dynamic(s, s->common_box);
  s->hero = hero;
  for (int i = 0; i < 4; ++i) {
    char type_key[] = "hero.skill0_type";
    type_key[10] = '0' + i;
    s->native_skill_types[i] = read_int(s, type_key, 0);
    char kind_key[] = "hero.skill0_selection_kind";
    kind_key[10] = '0' + i;
    s->native_selection_kinds[i] = read_int(s, kind_key, 0);
  }
  s->linked_controls = read_int(s, "hero.linked_controls", 0) != 0;
  GtkWidget *b = section(s->hero_box, hero == 25 ? "Персональные навыки Кагуры"
                                                 : "Персональные навыки");
  add_binding(s, b, hero == 25 ? "Раскрытие зонта Сэймэй · навык 1" : "Навык 1",
              "hero", "skill:0", 'Q', 0, s->linked_controls && read_int(s, "hero.skill0_type", 0) == 1, FALSE);
  add_binding(s, b, hero == 25 ? "Спешно улетающий зонт · с зонтом" : "Навык 2",
              "hero", "skill:1", 'W', 0, FALSE, FALSE);
  if (hero == 25)
    add_binding(s, b, "Спешно улетающий зонт · без зонта", "hero",
                "skill:1:2521", 'W', 1, FALSE, FALSE);
  if (s->linked_controls) {
    add_binding(s, b, "Комбо · навык 1 → навык 2", "hero", "combo:0", 'E', 0, TRUE, FALSE);
    add_binding(s, b, "Комбо · навык 2 → навык 1", "hero", "combo:1", 'R', 0, TRUE, FALSE);
    gtk_box_append(GTK_BOX(b), wrap_label("Комбо удерживается до отпускания клавиши. Направление задаётся движением персонажа.", "clbb-muted"));
  } else {
  add_binding(s, b, hero == 25 ? "Переворот инь-ян · обе формы" : "Навык 3",
              "hero", "skill:2", 'E', 1, FALSE, FALSE);
  if (hero != 25)
    add_binding(s, b, "Навык 4 · если доступен", "hero", "skill:3", 'R', 0,
                FALSE, FALSE);
  }
  bind_control(s, b, "Отмена навыка", "hero.cancel_key", 'T');
  gtk_box_append(GTK_BOX(b),
                 wrap_label("Одинаковые клавиши допустимы: активная форма "
                            "героя определяет навык. Режим «При отпускании» "
                            "сохраняет предпросмотр до отпускания или отмены.",
                            "clbb-muted"));
  GtkWidget *g = section(s->common_box, "Исключения для этого героя");
  gtk_box_append(GTK_BOX(g),
                 wrap_label("Переключатель «Своя» включает персональное "
                            "назначение вместо общей кнопки.",
                            "clbb-muted"));
  const char *slots[] = {"extra:0",      "extra:1",    "extra:2",
                         "extra:3",      "attack",     "attack:minion",
                         "attack:tower", "shop:quick", "upgrade:0",
                         "upgrade:1",    "upgrade:2",  "upgrade:3"};
  const char *titles[] = {
      "Возвращение",           "Регенерация",       "Боевой навык",
      "Дополнительная кнопка", "Основная атака",    "Атака по миньонам",
      "Атака по башням",       "Быстрая покупка",   "Прокачка навыка 1",
      "Прокачка навыка 2",     "Прокачка навыка 3", "Прокачка навыка 4"};
  int keys[] = {'B', 'D', 'F', 0x01000007, 32,  'A',
                'S', 'Z', '1', '2',        '3', '4'};
  for (guint i = 0; i < G_N_ELEMENTS(slots); ++i)
    add_binding(s, g, titles[i], "hero", slots[i], keys[i], 0, i != 2 && i != 3,
                TRUE);
  section_set_open(s->hero_box, keep_personal);
  section_set_open(s->common_box, keep_overrides);
}
static gboolean on_key(GtkEventControllerKey *controller, guint keyval,
                       guint keycode, GdkModifierType state, gpointer data) {
  (void)keycode;
  (void)state;
  ClbbSettings *s = data;
  if (s->capture) {
    Control *c = s->capture;
    if (keyval == GDK_KEY_Escape) {
      stop_capture(s);
      clbb_settings_refresh(s);
      return TRUE;
    }
    /* Use unmodified Latin group when the display can translate it. */
    GdkDisplay *d = gtk_widget_get_display(s->panel);
    guint latin = 0;
    int group = 0, level = 0;
    GdkModifierType consumed = 0;
    if (gdk_display_translate_key(d, keycode, 0, 0, &latin, &group, &level,
                                  &consumed) &&
        latin)
      keyval = latin;
    int code = keyval == GDK_KEY_BackSpace ? 0 : clbb_settings_keycode(keyval);
    if (!code && keyval != GDK_KEY_BackSpace)
      return TRUE;
    stop_capture(s);
    if (code && !strcmp(c->key, "debug.mark_shortcut")) code = clbb_settings_chord(code,state);
    write_int(s, c->key, code);
    clbb_settings_refresh(s);
    return TRUE;
  }
  if (keyval == GDK_KEY_Escape) {
    clbb_settings_hide(s);
    return TRUE;
  }
  int code = clbb_settings_keycode(keyval);
  if (code && code == read_int(s, "hotkeys.settings", KEY_F1 + 6)) {
    clbb_settings_hide(s);
    return TRUE;
  }
  (void)controller;
  return FALSE;
}
static gboolean on_mouse(GtkEventControllerLegacy *controller, GdkEvent *event,
                         gpointer data) {
  (void)controller;
  ClbbSettings *s = data;
  if (!s->capture)
    return FALSE;
  GdkEventType type = gdk_event_get_event_type(event);
  if (type != GDK_BUTTON_PRESS && type != GDK_BUTTON_RELEASE)
    return FALSE;
  guint b = gdk_button_event_get_button(event);
  if (b == 1)
    return FALSE;
  if (type == GDK_BUTTON_RELEASE)
    return TRUE;
  int code = b == 3              ? (KEY_MOUSE | 2)
             : b == 2            ? (KEY_MOUSE | 4)
             : b >= 8 && b <= 12 ? (KEY_MOUSE | (8 << (b - 8)))
                                 : 0;
  if (code) {
    Control *c = s->capture;
    if (!strcmp(c->key, "debug.mark_shortcut")) return TRUE;
    stop_capture(s);
    write_int(s, c->key, code);
    clbb_settings_refresh(s);
  }
  return TRUE;
}
static void close_clicked(GtkButton *b, gpointer data) {
  (void)b;
  clbb_settings_hide(data);
}
static void nav_selected(GtkListBox *list, GtkListBoxRow *row, gpointer data) {
  (void)list;
  if (!row)
    return;
  ClbbSettings *s = data;
  const char *names[] = {"main", "controls", "sound", "hotkeys"};
  int i = gtk_list_box_row_get_index(row);
  if (i >= 0 && i < (int)G_N_ELEMENTS(names)) {
    stop_capture(s);
    gtk_stack_set_visible_child_name(s->stack, names[i]);
    clbb_settings_refresh(s);
  }
}

static gboolean parse_resolution(const char *text, int *width, int *height) {
  if (!text || !*text)
    return FALSE;
  char *end = NULL;
  gint64 w = g_ascii_strtoll(text, &end, 10);
  if (end == text || (*end != 'x' && *end != 'X'))
    return FALSE;
  const char *start = end + 1;
  gint64 h = g_ascii_strtoll(start, &end, 10);
  if (end == start || *end || w < 256 || w > 8192 || h < 256 || h > 8192)
    return FALSE;
  *width = (int)w;
  *height = (int)h;
  return TRUE;
}
static void display_update_draft(ClbbSettings *s) {
  int w = gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(s->display_width));
  int h = gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(s->display_height));
  s->display_dirty =
      w != s->display_current_width || h != s->display_current_height;
  gtk_widget_set_sensitive(s->display_apply, s->display_dirty);
}
static void display_size_changed(GtkSpinButton *spin, gpointer data) {
  (void)spin;
  ClbbSettings *s = data;
  if (s->refreshing)
    return;
  s->refreshing = TRUE;
  gtk_drop_down_set_selected(GTK_DROP_DOWN(s->display_presets), 5);
  s->refreshing = FALSE;
  display_update_draft(s);
}
static void display_text_changed(GtkEditable *editable, gpointer data) {
  (void)editable;
  ClbbSettings *s = data;
  if (s->refreshing)
    return;
  s->refreshing = TRUE;
  gtk_drop_down_set_selected(GTK_DROP_DOWN(s->display_presets), 5);
  s->refreshing = FALSE;
  /* GtkSpinButton may retain uncommitted text until focus changes. Preserve
   * that text across source/status refreshes; Apply commits both fields. */
  s->display_dirty = TRUE;
  gtk_widget_set_sensitive(s->display_apply, TRUE);
}
static void display_preset_changed(GObject *object, GParamSpec *param,
                                   gpointer data) {
  (void)param;
  ClbbSettings *s = data;
  if (s->refreshing)
    return;
  guint choice = gtk_drop_down_get_selected(GTK_DROP_DOWN(object));
  int w = s->display_current_width, h = s->display_current_height;
  switch (choice) {
  case 0:
    break;
  case 1:
    w = 1280;
    h = 720;
    break;
  case 2:
    w = 1920;
    h = 1080;
    break;
  case 3:
    w = 2560;
    h = 1440;
    break;
  case 4: {
    char *saved = read_text(s, "display.imported_resolution", "");
    gboolean valid = parse_resolution(saved, &w, &h);
    g_free(saved);
    if (!valid) {
      gtk_label_set_text(GTK_LABEL(s->message),
                         "Сохранённое разрешение EWM не найдено");
      s->refreshing = TRUE;
      gtk_drop_down_set_selected(GTK_DROP_DOWN(object), 5);
      s->refreshing = FALSE;
      return;
    }
    break;
  }
  default:
    return;
  }
  s->refreshing = TRUE;
  gtk_spin_button_set_value(GTK_SPIN_BUTTON(s->display_width), w);
  gtk_spin_button_set_value(GTK_SPIN_BUTTON(s->display_height), h);
  s->refreshing = FALSE;
  display_update_draft(s);
}
static void display_apply_clicked(GtkButton *button, gpointer data) {
  (void)button;
  ClbbSettings *s = data;
  gtk_spin_button_update(GTK_SPIN_BUTTON(s->display_width));
  gtk_spin_button_update(GTK_SPIN_BUTTON(s->display_height));
  display_update_draft(s);
  if (!s->display_dirty)
    return;
  int w = gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(s->display_width));
  int h = gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(s->display_height));
  char value[32];
  g_snprintf(value, sizeof(value), "%dx%d", w, h);
  if (write_value(s, "display.resolution", value)) {
    s->display_dirty = FALSE;
    clbb_settings_refresh(s);
  } else {
    gtk_label_set_text(GTK_LABEL(s->message),
                       "Не удалось применить разрешение экрана");
  }
}
static const int fps_presets[] = {30, 60, 90, 120, 144, 165, 180, 240};
static guint fps_preset_index(int value) {
  for (guint i = 0; i < G_N_ELEMENTS(fps_presets); ++i)
    if (fps_presets[i] == value)
      return i;
  return G_N_ELEMENTS(fps_presets);
}
static void pacing_update_draft(ClbbSettings *s) {
  const char *text = gtk_editable_get_text(GTK_EDITABLE(s->fps_spin));
  char saved[16];
  g_snprintf(saved, sizeof(saved), "%d", s->fps_current);
  gboolean vsync = gtk_switch_get_active(GTK_SWITCH(s->vsync_switch));
  // Compare editable text too: it may not be committed to the spin's value yet.
  s->pacing_dirty = g_strcmp0(text, saved) != 0 || vsync != s->vsync_current;
  gtk_widget_set_sensitive(s->pacing_apply, s->pacing_dirty);
}
static void fps_value_changed(GtkSpinButton *spin, gpointer data) {
  ClbbSettings *s = data;
  if (s->refreshing)
    return;
  s->refreshing = TRUE;
  gtk_drop_down_set_selected(
      GTK_DROP_DOWN(s->fps_preset),
      fps_preset_index(gtk_spin_button_get_value_as_int(spin)));
  s->refreshing = FALSE;
  pacing_update_draft(s);
}
static void fps_text_changed(GtkEditable *editable, gpointer data) {
  (void)editable;
  ClbbSettings *s = data;
  if (s->refreshing)
    return;
  s->refreshing = TRUE;
  gtk_drop_down_set_selected(GTK_DROP_DOWN(s->fps_preset),
                             G_N_ELEMENTS(fps_presets));
  s->refreshing = FALSE;
  s->pacing_dirty = TRUE;
  gtk_widget_set_sensitive(s->pacing_apply, TRUE);
}
static void fps_preset_changed(GObject *object, GParamSpec *spec,
                               gpointer data) {
  (void)spec;
  ClbbSettings *s = data;
  if (s->refreshing)
    return;
  guint selected = gtk_drop_down_get_selected(GTK_DROP_DOWN(object));
  if (selected >= G_N_ELEMENTS(fps_presets))
    return;
  s->refreshing = TRUE;
  gtk_spin_button_set_value(GTK_SPIN_BUTTON(s->fps_spin),
                            fps_presets[selected]);
  s->refreshing = FALSE;
  pacing_update_draft(s);
}
static void vsync_changed(GObject *object, GParamSpec *spec, gpointer data) {
  (void)object;
  (void)spec;
  ClbbSettings *s = data;
  if (!s->refreshing)
    pacing_update_draft(s);
}
static void pacing_apply_clicked(GtkButton *button, gpointer data) {
  (void)button;
  ClbbSettings *s = data;
  gtk_spin_button_update(GTK_SPIN_BUTTON(s->fps_spin));
  pacing_update_draft(s);
  if (!s->pacing_dirty)
    return;
  int fps = gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(s->fps_spin));
  gboolean vsync = gtk_switch_get_active(GTK_SWITCH(s->vsync_switch));
  char value[40];
  g_snprintf(value, sizeof(value), "limit=%d;vsync=%d", fps, vsync ? 1 : 0);
  if (write_value(s, "display.frame_pacing", value)) {
    s->pacing_dirty = FALSE;
    clbb_settings_refresh(s);
  } else {
    gtk_label_set_text(GTK_LABEL(s->message),
                       "Не удалось применить лимит FPS и VSync");
  }
}
static void pacing_controls(ClbbSettings *s, GtkWidget *card) {
  GtkWidget *title = label("Частота кадров", "clbb-section-title");
  gtk_widget_set_margin_top(title, 18);
  gtk_box_append(GTK_BOX(card), title);
  GtkWidget *row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 12);
  gtk_widget_add_css_class(row, "clbb-row");
  GtkWidget *text = label("Лимит FPS", "clbb-label");
  gtk_widget_set_hexpand(text, TRUE);
  gtk_box_append(GTK_BOX(row), text);
  const char *choices[] = {"30 FPS",  "60 FPS",  "90 FPS",  "120 FPS",
                           "144 FPS", "165 FPS", "180 FPS", "240 FPS",
                           "Вручную", NULL};
  s->fps_preset = gtk_drop_down_new_from_strings(choices);
  g_object_set_data(G_OBJECT(s->fps_preset), "clbb-settings-key",
                    "display.fps_preset");
  g_signal_connect(s->fps_preset, "notify::selected",
                   G_CALLBACK(fps_preset_changed), s);
  gtk_box_append(GTK_BOX(row), s->fps_preset);
  s->fps_spin = gtk_spin_button_new_with_range(30, 240, 1);
  gtk_spin_button_set_numeric(GTK_SPIN_BUTTON(s->fps_spin), TRUE);
  gtk_widget_set_size_request(s->fps_spin, 95, -1);
  g_object_set_data(G_OBJECT(s->fps_spin), "clbb-settings-key",
                    "display.fps_limit");
  g_signal_connect(s->fps_spin, "changed", G_CALLBACK(fps_text_changed), s);
  g_signal_connect(s->fps_spin, "value-changed", G_CALLBACK(fps_value_changed),
                   s);
  gtk_box_append(GTK_BOX(row), s->fps_spin);
  gtk_box_append(GTK_BOX(card), row);
  row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 16);
  gtk_widget_add_css_class(row, "clbb-row");
  text = wrap_label("Вертикальная синхронизация · VSync", "clbb-label");
  gtk_widget_set_hexpand(text, TRUE);
  gtk_box_append(GTK_BOX(row), text);
  s->vsync_switch = gtk_switch_new();
  gtk_widget_set_valign(s->vsync_switch, GTK_ALIGN_CENTER);
  g_object_set_data(G_OBJECT(s->vsync_switch), "clbb-settings-key",
                    "display.vsync");
  g_signal_connect(s->vsync_switch, "notify::active", G_CALLBACK(vsync_changed),
                   s);
  gtk_box_append(GTK_BOX(row), s->vsync_switch);
  switch_control(s, card, "Показывать FPS", "display.show_fps", 0);
  s->pacing_apply = gtk_button_new_with_label("Применить FPS");
  gtk_widget_add_css_class(s->pacing_apply, "clbb-bind");
  g_signal_connect(s->pacing_apply, "clicked", G_CALLBACK(pacing_apply_clicked),
                   s);
  gtk_box_append(GTK_BOX(row), s->pacing_apply);
  gtk_box_append(GTK_BOX(card), row);
  s->pacing_current = label("", "clbb-info");
  gtk_box_append(GTK_BOX(card), s->pacing_current);
  gtk_box_append(
      GTK_BOX(card),
      wrap_label(
          "Это верхний предел эмулятора. Игра сохраняет собственный лимит FPS; "
          "фактическая частота зависит от производительности и экрана.",
          "clbb-muted"));
}
static void pacing_refresh(ClbbSettings *s) {
  s->fps_current = CLAMP(read_int(s, "display.fps_limit", 60), 30, 240);
  s->vsync_current = read_bool(s, "display.vsync", TRUE);
  char *current =
      g_strdup_printf("Сохранено: до %d FPS · VSync %s", s->fps_current,
                      s->vsync_current ? "вкл." : "выкл.");
  gtk_label_set_text(GTK_LABEL(s->pacing_current), current);
  g_free(current);
  if (!s->pacing_dirty) {
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(s->fps_spin), s->fps_current);
    gtk_drop_down_set_selected(GTK_DROP_DOWN(s->fps_preset),
                               fps_preset_index(s->fps_current));
    gtk_switch_set_active(GTK_SWITCH(s->vsync_switch), s->vsync_current);
    pacing_update_draft(s);
  }
}

static void display_card(ClbbSettings *s, GtkWidget *box) {
  GtkWidget *card = section(box, "Экран");
  GtkWidget *row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 16);
  gtk_widget_add_css_class(row, "clbb-row");
  GtkWidget *title = label("Разрешение Android", "clbb-label");
  gtk_widget_set_hexpand(title, TRUE);
  gtk_box_append(GTK_BOX(row), title);
  const char *presets[] = {"Текущее",     "1280 × 720", "1920 × 1080",
                           "2560 × 1440", "Из EWM",     "Вручную",
                           NULL};
  s->display_presets = gtk_drop_down_new_from_strings(presets);
  g_object_set_data(G_OBJECT(s->display_presets), "clbb-settings-key",
                    "display.preset");
  g_signal_connect(s->display_presets, "notify::selected",
                   G_CALLBACK(display_preset_changed), s);
  gtk_box_append(GTK_BOX(row), s->display_presets);
  gtk_box_append(GTK_BOX(card), row);
  row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 12);
  gtk_widget_add_css_class(row, "clbb-row");
  gtk_box_append(GTK_BOX(row), label("Ширина", "clbb-muted"));
  s->display_width = gtk_spin_button_new_with_range(256, 8192, 1);
  gtk_spin_button_set_numeric(GTK_SPIN_BUTTON(s->display_width), TRUE);
  gtk_widget_set_size_request(s->display_width, 110, -1);
  g_object_set_data(G_OBJECT(s->display_width), "clbb-settings-key",
                    "display.width");
  g_signal_connect(s->display_width, "changed",
                   G_CALLBACK(display_text_changed), s);
  g_signal_connect(s->display_width, "value-changed",
                   G_CALLBACK(display_size_changed), s);
  gtk_box_append(GTK_BOX(row), s->display_width);
  gtk_box_append(GTK_BOX(row), label("Высота", "clbb-muted"));
  s->display_height = gtk_spin_button_new_with_range(256, 8192, 1);
  gtk_spin_button_set_numeric(GTK_SPIN_BUTTON(s->display_height), TRUE);
  gtk_widget_set_size_request(s->display_height, 110, -1);
  g_object_set_data(G_OBJECT(s->display_height), "clbb-settings-key",
                    "display.height");
  g_signal_connect(s->display_height, "changed",
                   G_CALLBACK(display_text_changed), s);
  g_signal_connect(s->display_height, "value-changed",
                   G_CALLBACK(display_size_changed), s);
  gtk_box_append(GTK_BOX(row), s->display_height);
  GtkWidget *spacer = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
  gtk_widget_set_hexpand(spacer, TRUE);
  gtk_box_append(GTK_BOX(row), spacer);
  s->display_apply = gtk_button_new_with_label("Применить");
  gtk_widget_add_css_class(s->display_apply, "clbb-bind");
  g_signal_connect(s->display_apply, "clicked",
                   G_CALLBACK(display_apply_clicked), s);
  gtk_box_append(GTK_BOX(row), s->display_apply);
  gtk_box_append(GTK_BOX(card), row);
  s->display_current = label("", "clbb-info");
  gtk_box_append(GTK_BOX(card), s->display_current);
  gtk_box_append(
      GTK_BOX(card),
      wrap_label("Картинка вписывается в окно с сохранением пропорций.",
                 "clbb-muted"));
  pacing_controls(s, card);
}
static void update_clicked(GtkButton *button, gpointer data) {
  ClbbSettings *s = data;
  (void)button;
  if (s->callbacks.write) s->callbacks.write(s->callbacks.user, "updates.open", "1");
  clbb_settings_refresh(s);
}
static void display_refresh(ClbbSettings *s) {
  s->display_current_width =
      CLAMP(read_int(s, "display.width", 1280), 256, 8192);
  s->display_current_height =
      CLAMP(read_int(s, "display.height", 720), 256, 8192);
  char *current =
      g_strdup_printf("Текущее разрешение: %d × %d", s->display_current_width,
                      s->display_current_height);
  gtk_label_set_text(GTK_LABEL(s->display_current), current);
  g_free(current);
  char *saved = read_text(s, "display.imported_resolution", "");
  int w = 0, h = 0;
  char *name = parse_resolution(saved, &w, &h)
                   ? g_strdup_printf("Из EWM · %d × %d", w, h)
                   : g_strdup("Из EWM");
  GtkStringList *model = GTK_STRING_LIST(
      gtk_drop_down_get_model(GTK_DROP_DOWN(s->display_presets)));
  if (g_strcmp0(name, gtk_string_list_get_string(model, 4))) {
    guint selected =
        gtk_drop_down_get_selected(GTK_DROP_DOWN(s->display_presets));
    const char *additions[] = {name, NULL};
    gtk_string_list_splice(model, 4, 1, additions);
    gtk_drop_down_set_selected(GTK_DROP_DOWN(s->display_presets), selected);
  }
  g_free(name);
  g_free(saved);
  if (!s->display_dirty) {
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(s->display_width),
                              s->display_current_width);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(s->display_height),
                              s->display_current_height);
    gtk_drop_down_set_selected(GTK_DROP_DOWN(s->display_presets), 0);
  }
  if (!s->display_dirty)
    display_update_draft(s);
}

ClbbSettings *clbb_settings_create(GtkOverlay *overlay,
                                   const ClbbSettingsCallbacks *callbacks) {
  ClbbSettings *s = g_new0(ClbbSettings, 1);
  if (callbacks)
    s->callbacks = *callbacks;
  s->hero = -1;
  s->controls = g_ptr_array_new_with_free_func(control_free);
  GtkWidget *root = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
  s->panel = root;
  g_weak_ref_init(&s->previous_focus, NULL);
  gtk_widget_add_css_class(root, "clbb-settings");
  gtk_widget_add_css_class(root, "clbb-shell");
  gtk_widget_set_focusable(root, TRUE);
  gtk_widget_set_visible(root, FALSE);
  gtk_overlay_add_overlay(overlay, root);
  gtk_overlay_set_measure_overlay(overlay, root, FALSE);
  gtk_overlay_set_clip_overlay(overlay, root, TRUE);
  GtkCssProvider *p = gtk_css_provider_new();
  gtk_css_provider_load_from_string(p, css);
  gtk_style_context_add_provider_for_display(
      gtk_widget_get_display(GTK_WIDGET(overlay)), GTK_STYLE_PROVIDER(p),
      GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
  g_object_unref(p);
  GtkWidget *header = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 20);
  gtk_widget_add_css_class(header, "clbb-header");
  GtkWidget *branding = gtk_box_new(GTK_ORIENTATION_VERTICAL, 5);
  s->brand = label("CLBB", "clbb-brand");
  gtk_box_append(GTK_BOX(branding), s->brand);
  gtk_box_append(GTK_BOX(branding),
                 label("COMPUTER LEGENDS BANG BANG", "clbb-caption"));
  gtk_widget_set_hexpand(branding, TRUE);
  gtk_box_append(GTK_BOX(header), branding);
  GtkWidget *close = gtk_button_new_with_label("Вернуться в игру");
  gtk_widget_add_css_class(close, "clbb-close");
  g_signal_connect(close, "clicked", G_CALLBACK(close_clicked), s);
  gtk_box_append(GTK_BOX(header), close);
  gtk_box_append(GTK_BOX(root), header);
  GtkWidget *body = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
  gtk_widget_set_vexpand(body, TRUE);
  gtk_box_append(GTK_BOX(root), body);
  GtkWidget *sidebar = gtk_list_box_new();
  gtk_widget_add_css_class(sidebar, "clbb-sidebar");
  gtk_widget_set_size_request(sidebar, 185, -1);
  gtk_list_box_set_selection_mode(GTK_LIST_BOX(sidebar), GTK_SELECTION_SINGLE);
  const char *items[] = {"Основное", "Управление", "Звук",
                         "Горячие клавиши"};
  for (guint i = 0; i < G_N_ELEMENTS(items); ++i)
    gtk_list_box_append(GTK_LIST_BOX(sidebar),
                        label(items[i], "clbb-nav-label"));
  GtkWidget *sidebar_scroll = gtk_scrolled_window_new();
  gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(sidebar_scroll),
                                 GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
  gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(sidebar_scroll), sidebar);
  gtk_box_append(GTK_BOX(body), sidebar_scroll);
  s->stack = GTK_STACK(gtk_stack_new());
  gtk_stack_set_transition_type(s->stack, GTK_STACK_TRANSITION_TYPE_CROSSFADE);
  gtk_stack_set_transition_duration(s->stack, 90);
  gtk_widget_set_hexpand(GTK_WIDGET(s->stack), TRUE);
  gtk_box_append(GTK_BOX(body), GTK_WIDGET(s->stack));
  GtkWidget *b =
      page(s, "main", "Настройки", "Ваше управление, встроенное в игру.");
  GtkWidget *card = section(b, "Обновление CLBB");
  s->update_version = label("", "clbb-label");
  gtk_box_append(GTK_BOX(card), s->update_version);
  gtk_box_append(GTK_BOX(card), wrap_label("Игра закроется. Обновление продолжится в отдельном окне.", "clbb-muted"));
  GtkWidget *update_open=gtk_button_new_with_label("Обновить CLBB");
  gtk_widget_add_css_class(update_open,"clbb-bind");
  g_signal_connect(update_open,"clicked",G_CALLBACK(update_clicked),s);
  gtk_box_append(GTK_BOX(card),update_open);
  display_card(s,b);

  b = page(s, "controls", "Управление",
           "Передвижение, общие кнопки и навыки выбранного героя.");
  card = section(b, "Передвижение");
  bind_control(s, card, "Кнопка передвижения", "movement.button",
               KEY_MOUSE | 2);
  mode_control(s, card, "Короткое нажатие", "movement.click_mode", 0, TRUE);
  Control *c = new_control(s, card, "Дальность клика", "movement.click_distance_percent",
                           CONTROL_NUMBER, 100);
  s->movement_distance_row = c->row;
  GtkWidget *distance = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
  c->widget = gtk_spin_button_new_with_range(0, 500, 1);
  gtk_widget_set_size_request(c->widget, 100, -1);
  g_signal_connect(c->widget, "value-changed", G_CALLBACK(changed_number), c);
  gtk_box_append(GTK_BOX(distance), c->widget);
  gtk_box_append(GTK_BOX(distance), label("%", "clbb-muted"));
  gtk_box_append(GTK_BOX(c->row), distance);
  c = new_control(s, card, "Порог удержания", "movement.hold_ms",
                           CONTROL_NUMBER, 150);
  c->widget = gtk_spin_button_new_with_range(40, 600, 10);
  gtk_widget_set_size_request(c->widget, 115, -1);
  g_signal_connect(c->widget, "value-changed", G_CALLBACK(changed_number), c);
  gtk_box_append(GTK_BOX(c->row), c->widget);
  gtk_box_append(
      GTK_BOX(card),
      wrap_label("Герой сразу идёт к точке нажатия. Если держать кнопку дольше "
                 "порога, герой начинает следовать за курсором.",
                 "clbb-muted"));
  card = section(b, "Общие назначения");
  const char *slots[] = {"extra:0",      "extra:1",    "extra:2",
                         "extra:3",      "attack",     "attack:minion",
                         "attack:tower", "shop:quick", "upgrade:0",
                         "upgrade:1",    "upgrade:2",  "upgrade:3"};
  const char *titles[] = {
      "Возвращение",           "Регенерация",       "Боевой навык",
      "Дополнительная кнопка", "Основная атака",    "Атака по миньонам",
      "Атака по башням",       "Быстрая покупка",   "Прокачка навыка 1",
      "Прокачка навыка 2",     "Прокачка навыка 3", "Прокачка навыка 4"};
  int keys[] = {'B', 'D', 'F', 0x01000007, 32,  'A',
                'S', 'Z', '1', '2',        '3', '4'};
  for (guint i = 0; i < G_N_ELEMENTS(slots); ++i)
    add_binding(s, card, titles[i], "common", slots[i], keys[i], 0,
                i != 2 && i != 3, FALSE);
  s->hero_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
  gtk_box_append(GTK_BOX(b), s->hero_box);
  s->common_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
  gtk_box_append(GTK_BOX(b), s->common_box);
  b = page(s, "sound", "Звук", "Общий уровень, игровой звук и голосовой чат.");
  card = section(b, "Громкость CLBB");
  const char *volume_titles[] = {"Общий уровень", "Игра", "Голосовой чат"};
  const char *volume_keys[] = {"audio.volume", "audio.game_volume", "audio.voice_volume"};
  for (guint i = 0; i < G_N_ELEMENTS(volume_keys); ++i) {
    c = new_control(s, card, volume_titles[i], volume_keys[i], CONTROL_VOLUME, 100);
    c->widget = gtk_scale_new_with_range(GTK_ORIENTATION_HORIZONTAL, 0, 100, 1);
    gtk_scale_set_draw_value(GTK_SCALE(c->widget), TRUE);
    gtk_scale_set_value_pos(GTK_SCALE(c->widget), GTK_POS_RIGHT);
    gtk_widget_set_size_request(c->widget, 280, -1);
    g_signal_connect(c->widget, "value-changed", G_CALLBACK(changed_volume), c);
    gtk_box_append(GTK_BOX(c->row), c->widget);
  }
  switch_control(s, card, "Выключить звук", "audio.muted", 0);
  gtk_box_append(
      GTK_BOX(card),
      wrap_label("Общий уровень управляет обоими каналами. Голосовой чат — громкость собеседников; громкость микрофона не меняется.",
                 "clbb-muted"));
  card = section(b, "Аудиоустройства");
  gtk_box_append(GTK_BOX(card),wrap_label("Аудиовывод", "clbb-muted"));
  s->audio_output=gtk_drop_down_new(NULL,NULL);g_object_set_data(G_OBJECT(s->audio_output),"clbb-settings-key","audio.output_device");gtk_box_append(GTK_BOX(card),s->audio_output);
  gtk_box_append(GTK_BOX(card),wrap_label("Аудиовход · микрофон", "clbb-muted"));
  s->audio_input=gtk_drop_down_new(NULL,NULL);g_object_set_data(G_OBJECT(s->audio_input),"clbb-settings-key","audio.input_device");gtk_box_append(GTK_BOX(card),s->audio_input);
  g_signal_connect(s->audio_input,"notify::selected",G_CALLBACK(audio_selected),s);
  g_signal_connect(s->audio_output,"notify::selected",G_CALLBACK(audio_selected),s);
  GtkWidget *audio_scan=gtk_button_new_with_label("Обновить устройства");
  gtk_widget_add_css_class(audio_scan,"clbb-secondary");
  gtk_widget_set_halign(audio_scan,GTK_ALIGN_END);
  gtk_widget_set_margin_top(audio_scan,8);
  gtk_box_append(GTK_BOX(card),audio_scan);
  g_signal_connect(audio_scan,"clicked",G_CALLBACK(audio_refresh_clicked),s);
  s->audio_status=wrap_label("Поиск аудиоустройств…","clbb-muted");gtk_box_append(GTK_BOX(card),s->audio_status);
  gtk_box_append(GTK_BOX(card),wrap_label("Выбор применяется только к CLBB. Включите микрофон в самой игре для голосового чата.","clbb-muted"));
  b = page(s, "hotkeys", "Горячие клавиши",
           "Системные действия можно назначить на удобные кнопки.");
  card = section(b, "Окно и курсор");
  bind_control(s, card, "Открыть настройки", "hotkeys.settings", KEY_F1 + 6);
  bind_control(s, card, "Ограничить курсор окном игры", "hotkeys.confine",
               KEY_F1 + 11);
  bind_control(s, card, "Полноэкранный режим", "hotkeys.fullscreen",
               KEY_F1 + 10);
  gtk_box_append(
      GTK_BOX(card),
      wrap_label("Нажмите на назначение, затем нужную клавишу или ПКМ. Escape "
                 "отменяет ввод; Backspace очищает назначение. Системные "
                 "сочетания не передаются игре.",
                 "clbb-muted"));
  card = section(b, "Запись отладки управления");
  switch_control(s, card, "Запись отладки", "debug.recording", 0);
  switch_control(s, card, "Автоматические метки расхождений", "debug.auto_marks", 1);
  bind_control(s, card, "Поставить метку", "debug.mark_shortcut", 0x04000000 | 'S');
  gtk_box_append(GTK_BOX(card), wrap_label("По метке сохраняются 5 секунд до и 5 после: ввод, координаты, камера и результаты применения. Без меток окно хранится только в памяти. По умолчанию Ctrl+S; можно назначить Ctrl, Shift, Alt с клавишей. Автометки отмечают подозрения, а не доказывают ошибку. Активные фрагменты — до 256 МиБ; старые переносятся в archive.", "clbb-muted"));
  s->debug_status = wrap_label("Запись выключена", "clbb-muted");
  gtk_box_append(GTK_BOX(card), s->debug_status);
  GtkWidget *footer = gtk_box_new(GTK_ORIENTATION_VERTICAL, 3);
  gtk_widget_add_css_class(footer, "clbb-footer");
  s->capture_hint = wrap_label(
      "ЛКМ всегда управляет игрой · настройки сохраняются автоматически", NULL);
  s->message = wrap_label("", "clbb-accent");
  gtk_box_append(GTK_BOX(footer), s->capture_hint);
  gtk_box_append(GTK_BOX(footer), s->message);
  gtk_box_append(GTK_BOX(root), footer);
  GtkEventController *ec = gtk_event_controller_key_new();
  gtk_event_controller_set_propagation_phase(ec, GTK_PHASE_CAPTURE);
  g_signal_connect(ec, "key-pressed", G_CALLBACK(on_key), s);
  gtk_widget_add_controller(s->panel, ec);
  ec = gtk_event_controller_legacy_new();
  gtk_event_controller_set_propagation_phase(ec, GTK_PHASE_CAPTURE);
  g_signal_connect(ec, "event", G_CALLBACK(on_mouse), s);
  gtk_widget_add_controller(s->panel, ec);
  g_signal_connect(sidebar, "row-selected", G_CALLBACK(nav_selected), s);
  gtk_list_box_select_row(GTK_LIST_BOX(sidebar), gtk_list_box_get_row_at_index(
                                                     GTK_LIST_BOX(sidebar), 0));
  clbb_settings_refresh(s);
  return s;
}
void clbb_settings_refresh(ClbbSettings *s) {
  if (!s || s->refreshing)
    return;
  s->refreshing = TRUE;
  if (s->update_version) {
  char *update_text = read_text(s, "updates.version", "dev");
  char *update_label = g_strdup_printf("CLBB · %s", update_text);
  gtk_label_set_text(GTK_LABEL(s->update_version), update_label);
  g_free(update_label); g_free(update_text);
  }
  audio_refresh(s);
  display_refresh(s);
  pacing_refresh(s);
  int hero = read_int(s, "hero.id", 0);
  gboolean native_types_changed = FALSE;
  for (int i = 0; i < 4; ++i) {
    char type_key[] = "hero.skill0_type";
    type_key[10] = '0' + i;
    native_types_changed |= read_int(s, type_key, 0) != s->native_skill_types[i];
    char kind_key[] = "hero.skill0_selection_kind";
    kind_key[10] = '0' + i;
    native_types_changed |= read_int(s, kind_key, 0) != s->native_selection_kinds[i];
  }
  if (hero != s->hero || native_types_changed || (read_int(s, "hero.linked_controls", 0) != 0) != s->linked_controls)
    build_hero(s, hero);
  char *t = read_text(s, "name", "CLBB");
  gtk_label_set_text(GTK_LABEL(s->brand), t);
  g_free(t);
  t = read_text(s, "debug.status", "Запись выключена");
  gtk_label_set_text(GTK_LABEL(s->debug_status), t);
  g_free(t);
  for (guint i = 0; i < s->controls->len; ++i) {
    Control *c = g_ptr_array_index(s->controls, i);
    if (s->capture == c)
      continue;
    int n = read_int(s, c->key, c->fallback);
    switch (c->kind) {
    case CONTROL_BIND:
      if (!strcmp(c->key, "debug.mark_shortcut")) {
        char *base = key_name(n & 0x01ffffff);
        t = g_strdup_printf("%s%s%s%s", n & 0x04000000 ? "Ctrl+" : "", n & 0x02000000 ? "Shift+" : "", n & 0x08000000 ? "Alt+" : "", base);
        g_free(base);
      } else t = key_name(n);
      gtk_button_set_label(GTK_BUTTON(c->widget), t);
      g_free(t);
      break;
    case CONTROL_SWITCH:
      if (GTK_IS_TOGGLE_BUTTON(c->widget)) {
        gboolean active = read_bool(s, c->key, c->fallback);
        gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(c->widget), active);
        gtk_button_set_label(GTK_BUTTON(c->widget), active ? "ВКЛ · запись" : "ВЫКЛ · запись");
      } else gtk_switch_set_active(GTK_SWITCH(c->widget),
                                  read_bool(s, c->key, c->fallback));
      break;
    case CONTROL_NUMBER:
      gtk_spin_button_set_value(GTK_SPIN_BUTTON(c->widget), n);
      break;
    case CONTROL_MODE:
      if (c->battle_spell_mode) {
        gboolean plain = read_int(s, "common.battle_spell_id", 0) == 20080;
        if (plain != c->plain_spell_mode) {
          const char *const plain_modes[] = {"При отпускании", "Моментальное нажатие", NULL};
          const char *const aimed_modes[] = {"Моментальное прожатие", "Обычный навык", "Моментальный с наведением", "Прожатие с удержанием", NULL};
          GtkStringList *model = gtk_string_list_new(plain ? plain_modes : aimed_modes);
          gtk_drop_down_set_model(GTK_DROP_DOWN(c->widget), G_LIST_MODEL(model));
          g_object_unref(model);
          gboolean personal = g_str_has_prefix(c->key, "hero.");
          g_free(c->key);
          c->key = g_strdup_printf("%s.bindings.%s.cast_mode", personal ? "hero" : "common", plain ? "extra:2:20080" : "extra:2");
          c->plain_spell_mode = plain;
        }
        n = read_int(s, c->key, c->fallback);
      }
      if (c->battle_spell_mode && !c->plain_spell_mode) {
        static const guint spell_indices[] = {1, 2, 0, 3};
        gtk_drop_down_set_selected(GTK_DROP_DOWN(c->widget), spell_indices[CLAMP(n, 0, 3)]);
      } else {
        gtk_drop_down_set_selected(GTK_DROP_DOWN(c->widget), CLAMP(n, 0, 1));
      }
      break;
    case CONTROL_VOLUME:
      gtk_range_set_value(GTK_RANGE(c->widget), CLAMP(n, 0, 100));
      break;
    }
    if (c->override_key) {
      gboolean enabled = read_bool(s, c->override_key, FALSE);
      gtk_widget_set_sensitive(c->widget, enabled);
      if (c->override_widget)
        gtk_switch_set_active(GTK_SWITCH(c->override_widget), enabled);
    }
  }
  gtk_widget_set_visible(s->movement_distance_row,
                         read_int(s, "movement.click_mode", 0) == 0);
  s->refreshing = FALSE;
}
void clbb_settings_show(ClbbSettings *s) {
  if (!s || clbb_settings_is_visible(s))
    return;
  GtkRoot *root = gtk_widget_get_root(s->panel);
  if (root)
    g_weak_ref_set(&s->previous_focus, gtk_root_get_focus(root));
  clbb_settings_refresh(s);
  if (s->callbacks.visibility)
    s->callbacks.visibility(s->callbacks.user, TRUE);
  gtk_widget_set_visible(s->panel, TRUE);
  gtk_widget_grab_focus(s->panel);
}
void clbb_settings_hide(ClbbSettings *s) {
  if (!clbb_settings_is_visible(s))
    return;
  stop_capture(s);
  gtk_widget_set_visible(s->panel, FALSE);
  GtkWidget *previous = g_weak_ref_get(&s->previous_focus);
  if (previous) {
    if (gtk_widget_get_mapped(previous))
      gtk_widget_grab_focus(previous);
    g_object_unref(previous);
  }
  g_weak_ref_set(&s->previous_focus, NULL);
  if (s->callbacks.visibility)
    s->callbacks.visibility(s->callbacks.user, FALSE);
}
gboolean clbb_settings_is_visible(const ClbbSettings *s) {
  return s && gtk_widget_get_visible(s->panel);
}
GtkWidget *clbb_settings_panel(ClbbSettings *s) {
  return s ? s->panel : NULL;
}
void clbb_settings_destroy(ClbbSettings *s) {
  if (!s)
    return;
  clbb_settings_hide(s);
  gtk_overlay_remove_overlay(GTK_OVERLAY(gtk_widget_get_parent(s->panel)),
                             s->panel);
  g_weak_ref_clear(&s->previous_focus);
  g_ptr_array_unref(s->controls);
  if(s->audio_input_ids)g_ptr_array_unref(s->audio_input_ids);
  if(s->audio_output_ids)g_ptr_array_unref(s->audio_output_ids);
  g_free(s->audio_inventory[0]);g_free(s->audio_inventory[1]);
  g_free(s);
}

gboolean clbb_settings_is_capturing(const ClbbSettings *s) { return s && s->capture != NULL; }
