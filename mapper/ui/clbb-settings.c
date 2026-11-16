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
  gboolean battle_spell_mode;
  gboolean plain_spell_mode;
  char *visible_if;
  int visible_mode;
  gboolean visible_not;
  char *inherit_key;
  GtkWidget *inherit_reset, *inherit_status;
  GtkWidget *conflict_button;
} Control;

struct ClbbSettings {
  GtkWidget *debug_status;
  GtkWidget *account_picker, *account_name, *account_status;
  GtkWidget *account_add, *account_switch, *account_second, *account_reload;
  GtkWidget *account_rename, *account_rename_row, *account_rename_name, *account_rename_save;
  GPtrArray *account_ids, *account_labels;
  GArray *account_flags;
  gboolean account_busy;
  GtkWidget *audio_input, *audio_output, *audio_status;
  GPtrArray *audio_input_ids, *audio_output_ids;
  char *audio_inventory[2];
  GtkWidget *panel;
  GWeakRef previous_focus;
  ClbbSettingsCallbacks callbacks;
  GPtrArray *controls;
  GtkWidget *hero_box;
  GtkWidget *palette_search, *palette_tiles, *profile_picker;
  GtkWidget *palette_tabs[4];
  GArray *profile_ids;
  int palette_group, selected_profile, live_profile, active_spell;
  char *palette_selected;
  char *palette_hero_name;
  char *offline_forms;
  char *hero_layout;
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
  gboolean active_profile;
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
    ".clbb-binding-scope { background: #19161c; border: 1px solid #392832; border-radius: 5px; padding: 12px; }"
    ".clbb-settings .clbb-conflict { border: 1px solid #ff526f; color: #ff8197; background: #42232f; }"
    ".clbb-settings entry.clbb-account-entry, .clbb-settings dropdown.clbb-account-picker searchentry {"
    " background: #211b25; background-image: none; border: 1px solid #493341; border-radius: 7px;"
    " color: #fff4f6; box-shadow: none; outline: none; min-height: 35px; padding: 0 10px; }"
    ".clbb-settings entry.clbb-account-entry:focus-within,"
    ".clbb-settings dropdown.clbb-account-picker searchentry:focus-within { border-color: #ff5876; box-shadow: none; }"
    ".clbb-settings .clbb-account-entry text, .clbb-settings .clbb-account-picker searchentry text {"
    " background: transparent; color: #fff4f6; caret-color: #ff5876; }"
    ".clbb-settings .clbb-account-entry placeholder, .clbb-settings .clbb-account-picker searchentry placeholder { color: #c1a9b9; }"
    ".clbb-settings .clbb-account-picker searchentry image { color: #c1a9b9; }"
    ".clbb-settings .clbb-account-picker popover separator { background: #493341; }"
    ".clbb-palette-page .clbb-title { font-size: 32px; font-weight: 750; color: #fff4f6; }"
    ".clbb-palette-page .clbb-muted { color: #c1a9b9; }"
    ".clbb-palette-toolbar { margin-top: 34px; margin-bottom: 15px; }"
    ".clbb-palette-surface { background: #1a161e; border: 1px solid #493341; border-radius: 12px; padding: 18px; }"
    ".clbb-settings .clbb-palette-search, .clbb-settings .clbb-palette-picker,"
    ".clbb-settings button.clbb-palette-tab, .clbb-settings button.clbb-palette-tile {"
    " background: #211b25; background-image: none; color: #fff4f6;"
    " border: 1px solid #493341; border-radius: 7px; box-shadow: none; text-shadow: none; font-size: 16px; }"
    ".clbb-settings .clbb-palette-search { min-height: 40px; padding: 0 12px; }"
    ".clbb-settings .clbb-palette-search text { color: #fff4f6; }"
    ".clbb-settings .clbb-palette-search placeholder { color: #c1a9b9; }"
    ".clbb-settings .clbb-palette-picker { min-width: 145px; min-height: 40px; }"
    ".clbb-settings button.clbb-palette-tab { min-height: 40px; padding: 0 12px; }"
    ".clbb-settings button.clbb-palette-tile { min-height: 72px; min-width: 145px; padding: 0 12px; }"
    ".clbb-settings button.clbb-palette-tab:hover, .clbb-settings button.clbb-palette-tile:hover { border-color: #ff5876; }"
    ".clbb-settings button.clbb-palette-selected { background: #3d2230; border-color: #ff5876; color: #fff4f6; }"
    ".clbb-settings .clbb-palette-editor { border-top: 1px solid #493341; margin-top: 15px; padding-top: 16px; }"
    ".clbb-settings .clbb-palette-editor-title { font-size: 24px; font-weight: 750; color: #fff4f6; }"
    ".clbb-palette-editor .clbb-binding-scope { background: #151218; border: 1px solid #493341; border-radius: 8px; padding: 14px; }"
    ".clbb-palette-editor .clbb-row { padding: 11px 0; border-color: #493341; }"
    ".clbb-palette-editor .clbb-label { font-size: 16px; font-weight: 400; }"
    ".clbb-palette-editor .clbb-bind:not(.capture):not(.clbb-conflict),"
    ".clbb-palette-editor dropdown { background: #211b25; background-image: none; color: #fff4f6; border: 1px solid #493341; border-radius: 7px; font-size: 16px; font-weight: 400; }"
    ".clbb-palette-editor .clbb-bind { min-width: 26px; min-height: 34px; }"
    ".clbb-palette-editor dropdown { min-height: 34px; }"
    ".clbb-inherited, .clbb-settings .clbb-inherited label { color: #b4a8b3; }"
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
  g_autofree char *profile_key = s->selected_profile > 0 && g_str_has_prefix(key,"hero.")
      && (g_str_has_prefix(key,"hero.bindings.") || g_str_has_prefix(key,"hero.movement.")
          || g_str_has_prefix(key,"hero.cancel_key"))
      ? g_strdup_printf("profile.%d.%s",s->selected_profile,key) : NULL;
  const char *v =
      s->callbacks.read ? s->callbacks.read(s->callbacks.user, profile_key ? profile_key : key) : NULL;
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
  g_autofree char *profile_key = s->selected_profile > 0 && g_str_has_prefix(key,"hero.")
      && (g_str_has_prefix(key,"hero.bindings.") || g_str_has_prefix(key,"hero.movement.")
          || g_str_has_prefix(key,"hero.cancel_key"))
      ? g_strdup_printf("profile.%d.%s",s->selected_profile,key) : NULL;
  gboolean ok =
      s->callbacks.write && s->callbacks.write(s->callbacks.user, profile_key ? profile_key : key, value);
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
  if ((code & 0xffff0000) == 0x20000000) return g_strdup_printf("Клавиша · код %u",code & 0xffff);
  if ((code & 0xff000000) == 0x10000000) {
    guint symbol=code & 0xffffff;
    const char *name=gdk_keyval_name(symbol);
    if (!name || g_str_has_prefix(name,"0x")) {
      const char *vendor=gdk_keyval_name(symbol | 0x10000000);
      if(vendor && !g_str_has_prefix(vendor,"0x")) name=vendor;
    }
    return name ? g_strdup(name) : g_strdup_printf("Клавиша · 0x%X",symbol);
  }
  if (!code)
    return g_strdup("Не задана");
  if (code >= KEY_F1 && code <= KEY_F1 + 34)
    return g_strdup_printf("F%d", code - KEY_F1 + 1);
  if ((code & 0xffff0000) == 0x03000000 && (code & 0xffff) != 0)
    return g_strdup_printf("Мышь · кнопка %u", code & 0xffff);
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
    default: {
      guint mask=code & ~KEY_MOUSE;
      if (mask>=8 && !(mask&(mask-1))) {
        guint n=1;while(mask>1){mask>>=1;++n;}
        return g_strdup_printf("Мышь %u",n);
      }
      return g_strdup_printf("Мышь 0x%X",code & ~KEY_MOUSE);
    }
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

gint clbb_settings_mousecode(guint button) {
  if (button == 1) return KEY_MOUSE | 1;
  if (button == 2) return KEY_MOUSE | 4;
  if (button == 3) return KEY_MOUSE | 2;
  if (button >= 8 && button <= 29) return KEY_MOUSE | (1u << (button - 5));
  if ((button >= 4 && button <= 7) || (button >= 30 && button <= 65535))
    return 0x03000000 | button;
  return 0;
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
    return 0x10000000 | keyval;
  default: {
    gunichar u = gdk_keyval_to_unicode(keyval);
    if(u >= 32 && u < 0x110000) return (gint)g_unichar_toupper(u);
    if(keyval && keyval != GDK_KEY_VoidSymbol &&
       (!(keyval & 0xff000000) || (keyval & 0xff000000)==0x10000000))
      return 0x10000000 | (keyval & 0xffffff);
    return 0;
  }
  }
}
/* Preserve semantic non-text keys (notably F13 from remapped mice).
 * Only text is translated to the unmodified Latin group for stable Q/W/etc. */
gint clbb_settings_event_keycode(GdkDisplay *display, guint keyval, guint hardware) {
  if(gdk_keyval_to_unicode(keyval)>=32) {
    guint latin=0;int group=0,level=0;GdkModifierType consumed=0;
    if(hardware && gdk_display_translate_key(display,hardware,0,0,&latin,&group,&level,&consumed)
       && gdk_keyval_to_unicode(latin)>=32) keyval=latin;
  }
  int code=clbb_settings_keycode(keyval);
  return code ? code : hardware && hardware<=65535 ? (gint)(0x20000000u | hardware) : 0;
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
  g_free(c->visible_if);
  g_free(c->inherit_key);
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
  GtkWidget *caption=gtk_box_new(GTK_ORIENTATION_VERTICAL,3);
  gtk_widget_set_hexpand(caption,TRUE);
  gtk_box_append(GTK_BOX(caption),wrap_label(title,"clbb-label"));
  gtk_box_append(GTK_BOX(c->row),caption);
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
      "Нажмите клавишу или кнопку мыши · Esc — отмена · Backspace — очистить");
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
  if (write_int(c->owner,c->key,selected) && g_str_has_suffix(c->key,"movement.click_mode"))
    clbb_settings_refresh(c->owner);
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
  const char *const moves[] = {"По прямой", "С обходом препятствий", "Клавиатура", NULL};
  const char *const spells[] = {"Моментальное прожатие", "Обычный навык",
                                "Моментальный с наведением",
                                "Прожатие с удержанием", NULL};
  const char *spell_key=strstr(key,".bindings.spell:");
  int spell=spell_key?atoi(spell_key+strlen(".bindings.spell:")):0;
  c->battle_spell_mode=g_str_has_suffix(key,".bindings.extra:2.cast_mode")
      || (spell&&spell!=20030&&spell!=20040&&spell!=20080&&spell!=20190);
  c->widget = gtk_drop_down_new_from_strings(
      c->battle_spell_mode ? spells : movement ? moves : casts);
  g_signal_connect(c->widget, "notify::selected", G_CALLBACK(changed_mode), c);
  gtk_box_append(GTK_BOX(c->row), c->widget);
}
static void reset_inheritance(GtkButton *button, gpointer data) {
  (void)button;
  Control *c = data;
  stop_capture(c->owner);
  if (write_value(c->owner, c->inherit_key, "false")) clbb_settings_refresh(c->owner);
}
static void add_inheritance(Control *c) {
  c->inherit_key = g_strdup_printf("%s.override", c->key);
  if (gtk_widget_get_ancestor(c->row,GTK_TYPE_FLOW_BOX)) {
    c->inherit_status=label("Наследует общее","clbb-muted");
    gtk_box_append(GTK_BOX(gtk_widget_get_first_child(c->row)),c->inherit_status);
  }
  c->inherit_reset = gtk_button_new_from_icon_name("insert-link-symbolic");
  gtk_widget_add_css_class(c->inherit_reset, "clbb-secondary");
  g_signal_connect(c->inherit_reset, "clicked", G_CALLBACK(reset_inheritance), c);
  gtk_box_append(GTK_BOX(c->row), c->inherit_reset);
}
static int action_key(const char *slot) {
  if (!strcmp(slot,"ally:mathilda") || !strcmp(slot,"ally:johnson")) return 'V';
  if (!strcmp(slot,"item:active")) return 0x01000007;
  if (g_str_has_prefix(slot,"skill:")) { int n=slot[6]-'0'; return n>=0&&n<4 ? "QWER"[n] : 0; }
  if (!strcmp(slot,"combo:0")) return 'E';
  if (!strcmp(slot,"combo:1")) return 'R';
  const char *slots[]={"extra:0","extra:1","extra:2","extra:3","attack","attack:minion","attack:tower","shop:quick","upgrade:0","upgrade:1","upgrade:2","upgrade:3"};
  int keys[]={'B','D','F',0x01000007,32,'A','S','Z','1','2','3','4'};
  for (guint n=0;n<G_N_ELEMENTS(slots);++n) if (!strcmp(slot,slots[n])) return keys[n];
  return 0;
}
static gboolean valid_action(const char *slot) {
  if (g_str_has_prefix(slot,"skill:")) {
    if (slot[6]<'0'||slot[6]>'3') return FALSE;
    if (!slot[7]) return TRUE;
    if (slot[7]!=':'||!slot[8]||strlen(slot)>24) return FALSE;
    for (const char *p=slot+8;*p;++p) if (!g_ascii_isdigit(*p)) return FALSE;
    return g_ascii_strtoll(slot+8,NULL,10)>0;
  }
  return action_key(slot)!=0;
}
/* Display-only spell IDs: never used for input routing or geometry. */
static const char *battle_spell_name(int skill) {
  switch (skill) {
  case 20020: return "Возмездие";
  case 20030: return "Вдохновение";
  case 20040: return "Спринт";
  case 20050: return "Исцеление";
  case 20060: return "Эгида";
  case 20070: return "Оцепенение";
  case 20080: return "Очищение";
  case 20100: return "Вспышка";
  case 20140: return "Огненный выстрел";
  case 20150: return "Кара";
  case 20160: return "Прибытие";
  case 20190: return "Отмщение";
  default: return NULL;
  }
}
static const char *action_title(const char *slot, int skill, const char *config) {
  (void)config;
  if (!strcmp(slot,"item:active") || !strcmp(slot,"extra:3")) return "Активный предмет";
  if (!strcmp(slot,"ally:mathilda")) return "Помощь Матильды";
  if (!strcmp(slot,"skill:0")) return "Навык 1";
  if (!strcmp(slot,"skill:1")) return "Навык 2";
  if (!strcmp(slot,"skill:2")) return "Навык 3";
  if (!strcmp(slot,"skill:3")) return "Навык 4";
  if (!strcmp(slot,"combo:0")) return "Комбо · навык 1 → навык 2";
  if (!strcmp(slot,"combo:1")) return "Комбо · навык 2 → навык 1";
  const char *slots[]={"extra:0","extra:1","extra:2","extra:3","attack","attack:minion","attack:tower","shop:quick","upgrade:0","upgrade:1","upgrade:2","upgrade:3"};
  const char *titles[]={"Возвращение","Регенерация","Боевой навык","Дополнительная кнопка","Основная атака","Атака по миньонам","Атака по башням","Быстрая покупка","Прокачка навыка 1","Прокачка навыка 2","Прокачка навыка 3","Прокачка навыка 4"};
  (void)skill;
  for (guint n=0;n<G_N_ELEMENTS(slots);++n) if (!strcmp(slot,slots[n])) return titles[n];
  return "Кнопка";
}
static void action_scope(ClbbSettings *s,GtkWidget *flow,const char *prefix,const char *config,const char *mode_config,const char *slot,int type,gboolean cancel) {
  (void)type;
  gboolean personal=!strcmp(prefix,"hero");
  GtkWidget *box=gtk_box_new(GTK_ORIENTATION_VERTICAL,8);
  gtk_widget_add_css_class(box,"clbb-binding-scope");
  gtk_widget_set_hexpand(box,TRUE);
  gtk_flow_box_insert(GTK_FLOW_BOX(flow),box,-1);
  gtk_box_append(GTK_BOX(box),label(personal?(s->palette_hero_name?s->palette_hero_name:"Для этого героя"):"Для всех героев","clbb-section-title"));
  guint first=s->controls->len;
  const char *binding=g_str_has_prefix(config,"spell:")?config:slot;
  char *key=cancel?g_strdup_printf("%s.cancel_key",prefix):g_strdup_printf("%s.bindings.%s.key",prefix,binding);
  bind_control(s,box,"Клавиша",key,cancel?'T':action_key(slot));g_free(key);
  if (g_str_has_prefix(slot,"skill:") && strlen(slot)==7 && slot[6]>='0' && slot[6]<='3') {
    key=g_strdup_printf("%s.bindings.upgrade:%c.key",prefix,slot[6]);
    bind_control(s,box,"Прокачка навыка",key,'1'+slot[6]-'0');
    g_free(key);
  }
  gboolean fixed=g_str_has_prefix(slot,"combo:");
  gboolean held_only=((s->hero==126 && (!strcmp(slot,"skill:0")||!strcmp(slot,"skill:1")||!strcmp(slot,"skill:2"))) || (s->hero==78 && !strcmp(slot,"skill:0")));
  if (!cancel&&!fixed&&strcmp(slot,"match:statistics")) {
    GPtrArray *forms=g_ptr_array_new_with_free_func(g_free);
    g_ptr_array_add(forms,g_strdup(!strcmp(slot,"extra:2")?mode_config:slot));
    char **lines=g_strsplit(s->hero_layout,"\n",258);
    if (!held_only && g_str_has_prefix(slot,"skill:")) for (guint n=0;lines[n]&&forms->len<16;++n) {
      char **v=g_strsplit(lines[n],"\t",7);
      if (g_strv_length(v)==6&&!strcmp(v[0],slot)&&valid_action(v[1])&&g_str_has_prefix(v[1],slot)&&v[1][strlen(slot)]==':') {
        gboolean found=FALSE;for (guint k=0;k<forms->len;++k) found|=!strcmp(g_ptr_array_index(forms,k),v[1]);
        if (!found) g_ptr_array_add(forms,g_strdup(v[1]));
      }
      g_strfreev(v);
    }
    g_strfreev(lines);
    if(!held_only&&s->offline_forms&&g_str_has_prefix(slot,"skill:")) {
      g_auto(GStrv) saved=g_strsplit(s->offline_forms,"\n",-1);
      for(guint n=0;saved[n]&&forms->len<16;n++){
        g_auto(GStrv) cols=g_strsplit(saved[n],"\t",4);
        if(g_strv_length(cols)!=3||atoi(cols[0])!=s->hero||strcmp(cols[1],slot))continue;
        g_autofree char *form=g_strdup_printf("%s:%s",slot,cols[2]);
        if(valid_action(form)&&!g_ptr_array_find_with_equal_func(forms,form,g_str_equal,NULL))
          g_ptr_array_add(forms,g_steal_pointer(&form));
      }
    }
    for (guint n=0;n<forms->len;++n) {
      const char *form=g_ptr_array_index(forms,n);
      char *title=forms->len>1?g_strdup_printf("Нажатие · версия %u",n+1):g_strdup("Нажатие");
      if (held_only) {
        GtkWidget *row=gtk_box_new(GTK_ORIENTATION_HORIZONTAL,16);
        gtk_widget_add_css_class(row,"clbb-row");
        GtkWidget *caption=label("Нажатие","clbb-label");gtk_widget_set_hexpand(caption,TRUE);
        gtk_box_append(GTK_BOX(row),caption);
        gtk_box_append(GTK_BOX(row),label("При отпускании","clbb-muted"));
        gtk_box_append(GTK_BOX(box),row);
      } else {
        key=g_strdup_printf("%s.bindings.%s.cast_mode",prefix,form);
        mode_control(s,box,title,key,0,FALSE);g_free(key);
      }
      g_free(title);
    }
    if (g_str_has_prefix(slot,"skill:")||!strcmp(slot,"extra:2")) for (guint n=0;n<forms->len;++n) {
      const char *form=g_ptr_array_index(forms,n);
      char *title=forms->len>1?g_strdup_printf("Останавливать движение · версия %u",n+1):g_strdup("Останавливать движение");
      key=g_strdup_printf("%s.bindings.%s.stop_movement",prefix,form);
      switch_control(s,box,title,key,0);g_free(key);g_free(title);
    }
    g_ptr_array_unref(forms);
  }
  if (personal) for (guint n=first;n<s->controls->len;++n) add_inheritance(g_ptr_array_index(s->controls,n));
}
static void action_body(ClbbSettings *s,GtkWidget *content,const char *slot,const char *config,const char *mode_config,int type,gboolean cancel) {
  GtkWidget *flow=gtk_flow_box_new();
  gtk_flow_box_set_selection_mode(GTK_FLOW_BOX(flow),GTK_SELECTION_NONE);
  gtk_flow_box_set_min_children_per_line(GTK_FLOW_BOX(flow),1);
  gtk_flow_box_set_max_children_per_line(GTK_FLOW_BOX(flow),2);
  gtk_flow_box_set_column_spacing(GTK_FLOW_BOX(flow),12);
  gtk_flow_box_set_row_spacing(GTK_FLOW_BOX(flow),12);
  gtk_box_append(GTK_BOX(content),flow);
  action_scope(s,flow,"common",config,mode_config,slot,type,cancel);
  if (s->hero>0 && strcmp(slot,"match:statistics")) action_scope(s,flow,"hero",config,mode_config,slot,type,cancel);
  if(((s->hero==126 && (!strcmp(slot,"skill:0")||!strcmp(slot,"skill:1")||!strcmp(slot,"skill:2"))) || (s->hero==78 && !strcmp(slot,"skill:0"))))
    gtk_box_append(GTK_BOX(content),wrap_label("Результат навыка зависит от длительности удержания. Применение — только при отпускании.","clbb-muted"));
}
static void movement_scope(ClbbSettings *s,GtkWidget *flow,gboolean personal) {
  GtkWidget *box=gtk_box_new(GTK_ORIENTATION_VERTICAL,8);
  gtk_widget_add_css_class(box,"clbb-binding-scope");gtk_widget_set_hexpand(box,TRUE);
  gtk_flow_box_insert(GTK_FLOW_BOX(flow),box,-1);
  gtk_box_append(GTK_BOX(box),label(personal?(s->palette_hero_name?s->palette_hero_name:"Для этого героя"):"Для всех героев","clbb-section-title"));
  guint first=s->controls->len;
  const char *prefix=personal?"hero.movement":"movement";
  char *key=g_strdup_printf("%s.click_mode",prefix);mode_control(s,box,"Режим",key,0,TRUE);g_free(key);
  key=g_strdup_printf("%s.button",prefix);Control *mouse=bind_control(s,box,"Клавиша",key,KEY_MOUSE|2);g_free(key);
  mouse->visible_if=g_strdup_printf("%s.click_mode",prefix);mouse->visible_mode=2;mouse->visible_not=TRUE;
  key=g_strdup_printf("%s.click_distance_percent",prefix);
  Control *c=new_control(s,box,"Дальность клика, %",key,CONTROL_NUMBER,100);g_free(key);
  c->visible_if=g_strdup_printf("%s.click_mode",prefix);
  c->widget=gtk_spin_button_new_with_range(0,500,1);gtk_widget_set_size_request(c->widget,100,-1);
  g_signal_connect(c->widget,"value-changed",G_CALLBACK(changed_number),c);gtk_box_append(GTK_BOX(c->row),c->widget);
  if (!personal) s->movement_distance_row=c->row;
  key=g_strdup_printf("%s.hold_ms",prefix);c=new_control(s,box,"Порог удержания, мс",key,CONTROL_NUMBER,150);g_free(key);
  c->widget=gtk_spin_button_new_with_range(40,600,10);gtk_widget_set_size_request(c->widget,100,-1);
  g_signal_connect(c->widget,"value-changed",G_CALLBACK(changed_number),c);gtk_box_append(GTK_BOX(c->row),c->widget);
  c->visible_if=g_strdup_printf("%s.click_mode",prefix);c->visible_mode=2;c->visible_not=TRUE;
  const char *directions[]={"up","down","left","right"};
  const char *titles[]={"Вверх","Вниз","Влево","Вправо"};
  const int defaults[]={'W','S','A','D'};
  for (guint n=0;n<4;++n) {
    key=g_strdup_printf("%s.%s",prefix,directions[n]);c=bind_control(s,box,titles[n],key,defaults[n]);g_free(key);
    c->visible_if=g_strdup_printf("%s.click_mode",prefix);c->visible_mode=2;
  }
  if (personal) for (guint n=first;n<s->controls->len;++n) add_inheritance(g_ptr_array_index(s->controls,n));
}
static void clear_dynamic(ClbbSettings *s, GtkWidget *box) {
  for (gint i = (gint)s->controls->len - 1; i >= 0; --i) {
    Control *c = g_ptr_array_index(s->controls, i);
    if (gtk_widget_is_ancestor(c->row, box)) {
      if (s->capture == c)
        stop_capture(s);
      g_signal_handlers_disconnect_by_data(c->widget,c);
      if (c->inherit_reset) g_signal_handlers_disconnect_by_data(c->inherit_reset,c);
      g_ptr_array_remove_index(s->controls, i);
    }
  }
  GtkWidget *w;
  while ((w = gtk_widget_get_first_child(box)))
    gtk_box_remove(GTK_BOX(box), w);
}
enum { PALETTE_BASIC, PALETTE_SKILLS, PALETTE_SPELLS, PALETTE_TEMP };

static const char *follow_up(int hero,const char *slot) {
  if(hero==77&&!strcmp(slot,"skill:1"))return "Отмена стены";
  if(hero==9&&!strcmp(slot,"skill:2"))return "Завершить вращение";
  if(hero==21&&!strcmp(slot,"skill:1"))return "Возврат к теням";
  return "";
}

static void palette_update(ClbbSettings *s);

static void palette_tile_clicked(GtkButton *button, gpointer user) {
  ClbbSettings *s=user;
  const char *slot=g_object_get_data(G_OBJECT(button),"clbb-slot");
  g_free(s->palette_selected);
  s->palette_selected=g_strdup(slot);
  stop_capture(s);
  palette_update(s);
}

static void palette_update(ClbbSettings *s) {
  if(!s->palette_tiles) return;
  for(GtkWidget *tile=gtk_widget_get_first_child(s->palette_tiles);tile;
      tile=gtk_widget_get_first_child(s->palette_tiles))
    gtk_flow_box_remove(GTK_FLOW_BOX(s->palette_tiles),tile);
  g_autofree char *query=g_utf8_casefold(gtk_editable_get_text(GTK_EDITABLE(s->palette_search)),-1);
  GtkWidget *first=NULL,*chosen=NULL;
  for(GtkWidget *pane=gtk_widget_get_first_child(s->hero_box);pane;pane=gtk_widget_get_next_sibling(pane)) {
    const char *slot=g_object_get_data(G_OBJECT(pane),"clbb-slot");
    const char *title=g_object_get_data(G_OBJECT(pane),"clbb-title");
    const char *bind=g_object_get_data(G_OBJECT(pane),"clbb-key");
    int fallback=GPOINTER_TO_INT(g_object_get_data(G_OBJECT(pane),"clbb-fallback"));
    int group=GPOINTER_TO_INT(g_object_get_data(G_OBJECT(pane),"clbb-group"));
    g_autofree char *key=key_name(read_int(s,bind,fallback));
    g_autofree char *description=g_strdup_printf("%s · %s",title,key);
    const char *groups[]={"Базовые","Навыки героя","Боевые навыки","Временные"};
    const char *upgrade=g_str_has_prefix(slot,"skill:")?" Прокачка навыка":"";
    g_autofree char *haystack=g_strdup_printf("%s %s %s%s",description,groups[group],follow_up(s->hero,slot),upgrade);
    g_autofree char *text=g_utf8_casefold(haystack,-1);
    gboolean show=*query?strstr(text,query)!=NULL:group==s->palette_group;
    gtk_widget_set_visible(pane,FALSE);
    if(!show)continue;
    if(!first)first=pane;
    if(!g_strcmp0(slot,s->palette_selected))chosen=pane;
    GtkWidget *tile=gtk_button_new_with_label(description);
    gtk_widget_add_css_class(tile,"clbb-palette-tile");
    gtk_widget_set_hexpand(tile,TRUE);
    g_object_set_data_full(G_OBJECT(tile),"clbb-slot",g_strdup(slot),g_free);
    g_signal_connect(tile,"clicked",G_CALLBACK(palette_tile_clicked),s);
    gtk_flow_box_insert(GTK_FLOW_BOX(s->palette_tiles),tile,-1);
  }
  if(!chosen)chosen=first;
  if(chosen) {
    gtk_widget_set_visible(chosen,TRUE);
    const char *slot=g_object_get_data(G_OBJECT(chosen),"clbb-slot");
    for(GtkWidget *row=gtk_widget_get_first_child(s->palette_tiles);row;row=gtk_widget_get_next_sibling(row)) {
      GtkWidget *tile=gtk_flow_box_child_get_child(GTK_FLOW_BOX_CHILD(row));
      if(!g_strcmp0(slot,g_object_get_data(G_OBJECT(tile),"clbb-slot")))
        gtk_widget_add_css_class(tile,"clbb-palette-selected");
    }
  }
}
static void palette_search_changed(GtkEditable *editable, gpointer user) {
  (void)editable; palette_update(user);
}
static void palette_tab_clicked(GtkButton *button,gpointer user) {
  ClbbSettings *s=user;
  s->palette_group=GPOINTER_TO_INT(g_object_get_data(G_OBJECT(button),"clbb-group"));
  g_clear_pointer(&s->palette_selected,g_free);
  for(int i=0;i<4;i++) {
    if(i==s->palette_group)gtk_widget_add_css_class(s->palette_tabs[i],"clbb-palette-selected");
    else gtk_widget_remove_css_class(s->palette_tabs[i],"clbb-palette-selected");
  }
  stop_capture(s);
  gtk_editable_set_text(GTK_EDITABLE(s->palette_search),"");
  palette_update(s);
}
static GtkWidget *palette_pane(ClbbSettings *s,const char *slot,const char *title,int group,const char *bind,int fallback) {
  GtkWidget *pane=gtk_box_new(GTK_ORIENTATION_VERTICAL,14);
  gtk_widget_add_css_class(pane,"clbb-palette-editor");
  gtk_box_append(GTK_BOX(pane),label(title,"clbb-palette-editor-title"));
  g_object_set_data_full(G_OBJECT(pane),"clbb-slot",g_strdup(slot),g_free);
  g_object_set_data_full(G_OBJECT(pane),"clbb-title",g_strdup(title),g_free);
  g_object_set_data_full(G_OBJECT(pane),"clbb-key",g_strdup(bind),g_free);
  g_object_set_data(G_OBJECT(pane),"clbb-fallback",GINT_TO_POINTER(fallback));
  g_object_set_data(G_OBJECT(pane),"clbb-group",GINT_TO_POINTER(group));
  gtk_box_append(GTK_BOX(s->hero_box),pane);
  return pane;
}
static void palette_action(ClbbSettings *s,const char *slot,const char *config,const char *title,int group,int type) {
  gboolean cancel=!strcmp(slot,"cancel");
  const char *scope=s->hero&&!g_str_equal(slot,"match:statistics")?"hero":"common";
  g_autofree char *key=cancel?g_strdup_printf("%s.cancel_key",scope):g_strdup_printf("%s.bindings.%s.key",scope,config);
  GtkWidget *pane=palette_pane(s,config,title,group,key,cancel?'T':action_key(slot));
  action_body(s,pane,slot,config,config,type,cancel);
  const char *phase=follow_up(s->hero,slot);
  if(*phase){
    g_autofree char *note=g_strdup_printf("%s — клавиша этого навыка",phase);
    gtk_box_append(GTK_BOX(pane),wrap_label(note,"clbb-muted"));
  }
}
static void build_hero(ClbbSettings *s,int hero,const char *layout) {
  clear_dynamic(s,s->hero_box);
  s->hero=hero;g_free(s->hero_layout);s->hero_layout=g_strdup(layout);
  g_clear_pointer(&s->offline_forms,g_free);
  if(hero>0&&!*layout)s->offline_forms=read_text(s,"profiles.forms","");
  s->active_profile=hero>0;
  GtkWidget *pane=palette_pane(s,"movement","Передвижение",PALETTE_BASIC,
       hero?"hero.movement.button":"movement.button",KEY_MOUSE|2);
  GtkWidget *flow=gtk_flow_box_new();
  gtk_flow_box_set_selection_mode(GTK_FLOW_BOX(flow),GTK_SELECTION_NONE);
  gtk_flow_box_set_min_children_per_line(GTK_FLOW_BOX(flow),1);
  gtk_flow_box_set_max_children_per_line(GTK_FLOW_BOX(flow),2);
  gtk_flow_box_set_column_spacing(GTK_FLOW_BOX(flow),12);
  gtk_box_append(GTK_BOX(pane),flow);
  movement_scope(s,flow,FALSE);
  if(hero>0)movement_scope(s,flow,TRUE);
  const char *basic[]={"attack","attack:minion","attack:tower","extra:0","extra:1","cancel","match:statistics","shop:quick"};
  for(guint i=0;i<G_N_ELEMENTS(basic);i++) {
    const char *slot=basic[i];
    const char *title=!strcmp(slot,"cancel")?"Отмена навыка":
        !strcmp(slot,"match:statistics")?"Статистика":action_title(slot,0,slot);
    palette_action(s,slot,slot,title,PALETTE_BASIC,0);
  }
  for(int i=0;i<3;i++) {
    g_autofree char *slot=g_strdup_printf("skill:%d",i);
    palette_action(s,slot,slot,action_title(slot,0,slot),PALETTE_SKILLS,0);
  }
  g_auto(GStrv) lines=g_strsplit(layout,"\n",258);
  GHashTable *seen=g_hash_table_new_full(g_str_hash,g_str_equal,g_free,NULL);
  for(guint i=0;lines[i];i++){
    g_auto(GStrv) cols=g_strsplit(lines[i],"\t",7);
    if(g_strv_length(cols)!=6||!valid_action(cols[0])||
       (strcmp(cols[0],"skill:3")&&!g_str_has_prefix(cols[0],"combo:"))||
       !g_hash_table_add(seen,g_strdup(cols[0])))continue;
    palette_action(s,cols[0],cols[0],action_title(cols[0],atoi(cols[3]),cols[0]),PALETTE_SKILLS,atoi(cols[5]));
  }
  g_hash_table_unref(seen);
  const int spells[]={20020,20030,20040,20050,20060,20070,20080,20100,20140,20150,20160,20190};
  for(int pass=0;pass<2;pass++)for(guint i=0;i<G_N_ELEMENTS(spells);i++) {
    int id=spells[i];
    if((pass==0)!=(id==s->active_spell && hero==s->live_profile))continue;
    g_autofree char *slot=g_strdup_printf("spell:%d",id);
    g_autofree char *title=pass?g_strdup(battle_spell_name(id)):g_strdup_printf("%s · Активен",battle_spell_name(id));
    palette_action(s,"extra:2",slot,title,PALETTE_SPELLS,0);
  }
  palette_action(s,"ally:johnson","ally:johnson","Посадка к Джонсону",PALETTE_TEMP,1);
  palette_action(s,"ally:mathilda","ally:mathilda","Помощь Матильды",PALETTE_TEMP,1);
  palette_action(s,"item:active","item:active","Активный предмет",PALETTE_TEMP,1);
  palette_update(s);
}
static void palette_pick_profile(GObject *picker,GParamSpec *spec,gpointer user) {
  (void)spec;
  ClbbSettings *s=user;
  guint index=gtk_drop_down_get_selected(GTK_DROP_DOWN(picker));
  if(index>=s->profile_ids->len)return;
  s->selected_profile=g_array_index(s->profile_ids,int,index);
  stop_capture(s);
  clbb_settings_refresh(s);
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
    if (!strcmp(c->key,"debug.mark_shortcut") &&
        (keyval==GDK_KEY_Shift_L || keyval==GDK_KEY_Shift_R ||
         keyval==GDK_KEY_Control_L || keyval==GDK_KEY_Control_R ||
         keyval==GDK_KEY_Alt_L || keyval==GDK_KEY_Alt_R)) return TRUE;
    int code = keyval == GDK_KEY_BackSpace ? 0 :
      clbb_settings_event_keycode(gtk_widget_get_display(s->panel),keyval,keycode);
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
  int code = clbb_settings_mousecode(b);
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
enum { ACCOUNT_OPEN = 1, ACCOUNT_CURRENT = 2 };

static void accounts_update_buttons(ClbbSettings *s) {
  guint index=gtk_drop_down_get_selected(GTK_DROP_DOWN(s->account_picker));
  gboolean valid=index<s->account_ids->len;
  guint flags=valid?g_array_index(s->account_flags,guint,index):0;
  gboolean other=!s->account_busy&&valid&&!(flags&ACCOUNT_CURRENT);
  gtk_widget_set_sensitive(s->account_switch,other);
  gtk_widget_set_sensitive(s->account_second,other&&!(flags&ACCOUNT_OPEN));
  gtk_widget_set_sensitive(s->account_add,!s->account_busy);
  gtk_widget_set_sensitive(s->account_reload,!s->account_busy);
  gtk_widget_set_sensitive(s->account_rename,valid&&!s->account_busy);
  gtk_widget_set_sensitive(s->account_rename_save,valid&&!s->account_busy);
}
static void accounts_selected(GObject *picker,GParamSpec *spec,gpointer user) {
  (void)picker;(void)spec;
  ClbbSettings *s=user;
  gtk_widget_set_visible(s->account_rename_row,FALSE);
  accounts_update_buttons(s);
}
static void accounts_request(ClbbSettings *s,const char *verb,const char *target);

static void accounts_worker(GTask *task,gpointer source,gpointer request,GCancellable *cancel) {
  (void)source;
  const char *configured=g_getenv("CLBB_ACCOUNTS_SOCKET");
  g_autofree char *path=configured?g_strdup(configured):
      g_build_filename(g_get_home_dir(),".local","share","clbb-debug-instances",".accounts.sock",NULL);
  g_autoptr(GSocketAddress) address=g_unix_socket_address_new(path);
  g_autoptr(GSocketClient) client=g_socket_client_new();
  g_socket_client_set_timeout(client,45);
  g_autoptr(GError) error=NULL;
  g_autoptr(GSocketConnection) connection=g_socket_client_connect(client,G_SOCKET_CONNECTABLE(address),cancel,&error);
  if(!connection){
    g_task_return_new_error(task,G_IO_ERROR,G_IO_ERROR_NOT_CONNECTED,
        "Служба аккаунтов недоступна. Запустите CLBB с обычного ярлыка.");
    return;
  }
  const char *wire=request;
  GOutputStream *output=g_io_stream_get_output_stream(G_IO_STREAM(connection));
  if(!g_output_stream_write_all(output,wire,strlen(wire),NULL,cancel,&error)){
    g_task_return_error(task,g_steal_pointer(&error));return;
  }
  GInputStream *input=g_io_stream_get_input_stream(G_IO_STREAM(connection));
  g_autoptr(GString) reply=g_string_new(NULL);
  char chunk[2048];
  gssize size;
  while((size=g_input_stream_read(input,chunk,sizeof(chunk),cancel,&error))>0){
    g_string_append_len(reply,chunk,size);
    if(reply->len>32768){
      g_task_return_new_error(task,G_IO_ERROR,G_IO_ERROR_INVALID_DATA,"Ответ службы аккаунтов слишком длинный.");
      return;
    }
  }
  if(size<0){g_task_return_error(task,g_steal_pointer(&error));return;}
  if(!g_utf8_validate(reply->str,-1,NULL)||!g_str_has_prefix(reply->str,"OK\n")){
    const char *message=g_utf8_validate(reply->str,-1,NULL)&&g_str_has_prefix(reply->str,"ERR\n")
        ?reply->str+4:"Не удалось выполнить действие с аккаунтом.";
    g_task_return_new_error(task,G_IO_ERROR,G_IO_ERROR_FAILED,"%s",message);
    return;
  }
  g_task_return_pointer(task,g_strdup(reply->str+3),g_free);
}
static void accounts_finished(GObject *source,GAsyncResult *result,gpointer ignored) {
  (void)ignored;
  ClbbSettings *s=g_object_get_data(source,"clbb-accounts-owner");
  if(!s)return;
  g_autoptr(GError) error=NULL;
  g_autofree char *reply=g_task_propagate_pointer(G_TASK(result),&error);
  const char *request=g_task_get_task_data(G_TASK(result));
  s->account_busy=FALSE;
  if(!reply)gtk_label_set_text(GTK_LABEL(s->account_status),error->message);
  else if(!strcmp(request,"list\n")){
    guint old=gtk_drop_down_get_selected(GTK_DROP_DOWN(s->account_picker));
    g_autofree char *selected=old<s->account_ids->len?g_strdup(g_ptr_array_index(s->account_ids,old)):NULL;
    g_ptr_array_set_size(s->account_ids,0);
    g_ptr_array_set_size(s->account_labels,0);
    g_array_set_size(s->account_flags,0);
    g_autoptr(GtkStringList) options=gtk_string_list_new(NULL);
    g_auto(GStrv) rows=g_strsplit(reply,"\n",-1);
    guint preferred=GTK_INVALID_LIST_POSITION,current=GTK_INVALID_LIST_POSITION;
    for(guint i=0;rows[i];++i){
      g_auto(GStrv) fields=g_strsplit(rows[i],"\t",5);
      if(g_strv_length(fields)!=4)continue;
      guint flags=(!strcmp(fields[2],"1")?ACCOUNT_OPEN:0)
                  | (!strcmp(fields[3],"1")?ACCOUNT_CURRENT:0);
      if(!g_strcmp0(fields[0],selected))preferred=s->account_ids->len;
      if(flags&ACCOUNT_CURRENT)current=s->account_ids->len;
      g_autofree char *title=g_strconcat(fields[1],flags&ACCOUNT_CURRENT?" · текущий":
          flags&ACCOUNT_OPEN?" · открыт":"",NULL);
      gtk_string_list_append(options,title);
      g_ptr_array_add(s->account_ids,g_strdup(fields[0]));
      g_ptr_array_add(s->account_labels,g_strdup(fields[1]));
      g_array_append_val(s->account_flags,flags);
    }
    gtk_drop_down_set_model(GTK_DROP_DOWN(s->account_picker),G_LIST_MODEL(options));
    gtk_drop_down_set_selected(GTK_DROP_DOWN(s->account_picker),
        preferred!=GTK_INVALID_LIST_POSITION?preferred:current);
  }else{
    gtk_label_set_text(GTK_LABEL(s->account_status),reply);
    if(g_str_has_prefix(request,"add\t"))gtk_editable_set_text(GTK_EDITABLE(s->account_name),"");
    if(g_str_has_prefix(request,"rename\t"))gtk_widget_set_visible(s->account_rename_row,FALSE);
    accounts_request(s,"list",NULL);
  }
  accounts_update_buttons(s);
}
static void accounts_request(ClbbSettings *s,const char *verb,const char *target) {
  if(s->account_busy)return;
  s->account_busy=TRUE;
  accounts_update_buttons(s);
  g_autoptr(GTask) task=g_task_new(s->panel,NULL,accounts_finished,NULL);
  g_task_set_task_data(task,target?g_strdup_printf("%s\t%s\n",verb,target):g_strdup_printf("%s\n",verb),g_free);
  g_task_run_in_thread(task,accounts_worker);
}
static void accounts_clicked(GtkButton *button,gpointer user) {
  ClbbSettings *s=user;
  GtkWidget *clicked=GTK_WIDGET(button);
  if(clicked==s->account_reload){accounts_request(s,"list",NULL);return;}
  if(clicked==s->account_add){
    const char *name=gtk_editable_get_text(GTK_EDITABLE(s->account_name));
    if(!*name){gtk_label_set_text(GTK_LABEL(s->account_status),"Введите название аккаунта.");return;}
    gtk_label_set_text(GTK_LABEL(s->account_status),"Подготавливаю новый профиль…");
    accounts_request(s,"add",name);
    return;
  }
  guint index=gtk_drop_down_get_selected(GTK_DROP_DOWN(s->account_picker));
  if(index>=s->account_ids->len)return;
  if(clicked==s->account_rename){
    gtk_editable_set_text(GTK_EDITABLE(s->account_rename_name),g_ptr_array_index(s->account_labels,index));
    gtk_widget_set_visible(s->account_rename_row,TRUE);
    gtk_widget_grab_focus(s->account_rename_name);
    gtk_editable_select_region(GTK_EDITABLE(s->account_rename_name),0,-1);
    return;
  }
  if(clicked==s->account_rename_save){
    const char *name=gtk_editable_get_text(GTK_EDITABLE(s->account_rename_name));
    if(!*name){gtk_label_set_text(GTK_LABEL(s->account_status),"Введите новое название.");return;}
    g_autofree char *target=g_strdup_printf("%s\t%s",(char *)g_ptr_array_index(s->account_ids,index),name);
    accounts_request(s,"rename",target);
    return;
  }
  gboolean switching=clicked==s->account_switch;
  gtk_label_set_text(GTK_LABEL(s->account_status),switching?"Переключаю аккаунт…":"Запускаю второе окно…");
  accounts_request(s,switching?"switch":"second",g_ptr_array_index(s->account_ids,index));
}
static void accounts_rename_enter(GtkEntry *entry,gpointer user) {
  (void)entry;
  ClbbSettings *s=user;
  accounts_clicked(GTK_BUTTON(s->account_rename_save),s);
}
static void accounts_page(ClbbSettings *s) {
  GtkWidget *page_box=page(s,"accounts","Аккаунты",
      "Выберите аккаунт для текущего или второго окна CLBB.");
  GtkWidget *card=gtk_box_new(GTK_ORIENTATION_VERTICAL,12);
  gtk_widget_add_css_class(card,"clbb-section");
  gtk_box_append(GTK_BOX(page_box),card);
  s->account_ids=g_ptr_array_new_with_free_func(g_free);
  s->account_labels=g_ptr_array_new_with_free_func(g_free);
  s->account_flags=g_array_new(FALSE,FALSE,sizeof(guint));
  s->account_picker=gtk_drop_down_new(NULL,NULL);
  gtk_widget_add_css_class(s->account_picker,"clbb-account-picker");
  gtk_drop_down_set_enable_search(GTK_DROP_DOWN(s->account_picker),TRUE);
  gtk_box_append(GTK_BOX(card),s->account_picker);
  GtkWidget *actions=gtk_box_new(GTK_ORIENTATION_HORIZONTAL,8);
  s->account_switch=gtk_button_new_with_label("Сменить аккаунт");
  s->account_second=gtk_button_new_with_label("Запустить второе окно с этим аккаунтом");
  s->account_reload=gtk_button_new_with_label("Обновить список");
  s->account_rename=gtk_button_new_with_label("Переименовать");
  gtk_box_append(GTK_BOX(actions),s->account_switch);
  gtk_box_append(GTK_BOX(actions),s->account_second);
  gtk_box_append(GTK_BOX(actions),s->account_rename);
  gtk_box_append(GTK_BOX(actions),s->account_reload);
  gtk_box_append(GTK_BOX(card),actions);
  s->account_rename_row=gtk_box_new(GTK_ORIENTATION_HORIZONTAL,8);
  s->account_rename_name=gtk_entry_new();
  gtk_widget_add_css_class(s->account_rename_name,"clbb-account-entry");
  gtk_entry_set_max_length(GTK_ENTRY(s->account_rename_name),48);
  gtk_entry_set_placeholder_text(GTK_ENTRY(s->account_rename_name),"Новое название");
  gtk_widget_set_hexpand(s->account_rename_name,TRUE);
  s->account_rename_save=gtk_button_new_with_label("Сохранить название");
  gtk_box_append(GTK_BOX(s->account_rename_row),s->account_rename_name);
  gtk_box_append(GTK_BOX(s->account_rename_row),s->account_rename_save);
  gtk_box_append(GTK_BOX(card),s->account_rename_row);
  gtk_widget_set_visible(s->account_rename_row,FALSE);
  g_signal_connect(s->account_rename_name,"activate",G_CALLBACK(accounts_rename_enter),s);
  GtkWidget *add=gtk_box_new(GTK_ORIENTATION_HORIZONTAL,8);
  s->account_name=gtk_entry_new();
  gtk_widget_add_css_class(s->account_name,"clbb-account-entry");
  gtk_entry_set_placeholder_text(GTK_ENTRY(s->account_name),"Название нового аккаунта");
  gtk_entry_set_max_length(GTK_ENTRY(s->account_name),48);
  gtk_widget_set_hexpand(s->account_name,TRUE);
  s->account_add=gtk_button_new_with_label("Добавить аккаунт");
  gtk_box_append(GTK_BOX(add),s->account_name);
  gtk_box_append(GTK_BOX(add),s->account_add);
  gtk_box_append(GTK_BOX(card),add);
  gtk_box_append(GTK_BOX(card),wrap_label("В новом окне войдите через меню самой игры.","clbb-muted"));
  s->account_status=wrap_label("","clbb-accent");
  gtk_box_append(GTK_BOX(card),s->account_status);
  GtkWidget *buttons[]={s->account_switch,s->account_second,s->account_reload,s->account_add,s->account_rename,s->account_rename_save};
  for(guint i=0;i<G_N_ELEMENTS(buttons);++i){
    gtk_widget_add_css_class(buttons[i],"clbb-secondary");
    g_signal_connect(buttons[i],"clicked",G_CALLBACK(accounts_clicked),s);
  }
  g_object_set_data(G_OBJECT(s->panel),"clbb-accounts-owner",s);
  g_signal_connect(s->account_picker,"notify::selected",G_CALLBACK(accounts_selected),s);
  accounts_update_buttons(s);
}
static void nav_selected(GtkListBox *list, GtkListBoxRow *row, gpointer data) {
  (void)list;
  if (!row)
    return;
  ClbbSettings *s = data;
  const char *names[] = {"main", "controls", "sound", "hotkeys", "accounts"};
  int i = gtk_list_box_row_get_index(row);
  if (i >= 0 && i < (int)G_N_ELEMENTS(names)) {
    stop_capture(s);
    gtk_stack_set_visible_child_name(s->stack, names[i]);
    if(!strcmp(names[i],"accounts"))accounts_request(s,"list",NULL);
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
  const char *items[] = {"Основное", "Герой", "Звук",
                         "Горячие клавиши", "Аккаунты"};
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

  b = page(s, "controls", "Настройки управления",
           "Выберите действие, чтобы изменить общий бинд или задать исключение для героя.");
  gtk_widget_add_css_class(b,"clbb-palette-page");
  GtkWidget *toolbar=gtk_box_new(GTK_ORIENTATION_HORIZONTAL,10);
  gtk_widget_add_css_class(toolbar,"clbb-palette-toolbar");
  s->palette_search=gtk_search_entry_new();
  gtk_widget_add_css_class(s->palette_search,"clbb-palette-search");
  gtk_widget_set_hexpand(s->palette_search,TRUE);
  g_object_set(s->palette_search,"placeholder-text","Найти действие или клавишу",NULL);
  gtk_box_append(GTK_BOX(toolbar),s->palette_search);
  GtkStringList *profiles=gtk_string_list_new(NULL);
  gtk_string_list_append(profiles,"Текущий герой");
  s->profile_ids=g_array_new(FALSE,FALSE,sizeof(int));
  int automatic=0;
  g_array_append_val(s->profile_ids,automatic);
  g_autofree char *catalog=read_text(s,"profiles.catalog","");
  g_auto(GStrv) rows=g_strsplit(catalog,"\n",-1);
  for(guint i=0;rows[i];i++){
    char *separator=strchr(rows[i],'\t');
    if(!separator)continue;
    int id=atoi(rows[i]);
    if(id<1||id>134)continue;
    gtk_string_list_append(profiles,separator+1);
    g_array_append_val(s->profile_ids,id);
  }
  s->profile_picker=gtk_drop_down_new(G_LIST_MODEL(profiles),NULL);
  gtk_widget_add_css_class(s->profile_picker,"clbb-palette-picker");
  gtk_drop_down_set_enable_search(GTK_DROP_DOWN(s->profile_picker),TRUE);
  gtk_box_append(GTK_BOX(toolbar),s->profile_picker);
  gtk_box_append(GTK_BOX(b),toolbar);
  GtkWidget *surface=gtk_box_new(GTK_ORIENTATION_VERTICAL,10);
  gtk_widget_add_css_class(surface,"clbb-palette-surface");
  gtk_box_append(GTK_BOX(b),surface);
  GtkWidget *tabs=gtk_box_new(GTK_ORIENTATION_HORIZONTAL,8);
  const char *tab_titles[]={"Базовые","Навыки героя","Боевые навыки","Временные"};
  for(int i=0;i<4;i++){
    s->palette_tabs[i]=gtk_button_new_with_label(tab_titles[i]);
    gtk_widget_add_css_class(s->palette_tabs[i],"clbb-palette-tab");
    g_object_set_data(G_OBJECT(s->palette_tabs[i]),"clbb-group",GINT_TO_POINTER(i));
    g_signal_connect(s->palette_tabs[i],"clicked",G_CALLBACK(palette_tab_clicked),s);
    gtk_box_append(GTK_BOX(tabs),s->palette_tabs[i]);
  }
  s->palette_group=PALETTE_SKILLS;
  gtk_widget_add_css_class(s->palette_tabs[s->palette_group],"clbb-palette-selected");
  gtk_box_append(GTK_BOX(surface),tabs);
  s->palette_tiles=gtk_flow_box_new();
  gtk_flow_box_set_selection_mode(GTK_FLOW_BOX(s->palette_tiles),GTK_SELECTION_NONE);
  gtk_flow_box_set_homogeneous(GTK_FLOW_BOX(s->palette_tiles),TRUE);
  gtk_flow_box_set_min_children_per_line(GTK_FLOW_BOX(s->palette_tiles),1);
  gtk_flow_box_set_max_children_per_line(GTK_FLOW_BOX(s->palette_tiles),8);
  gtk_flow_box_set_column_spacing(GTK_FLOW_BOX(s->palette_tiles),10);
  gtk_flow_box_set_row_spacing(GTK_FLOW_BOX(s->palette_tiles),10);
  gtk_box_append(GTK_BOX(surface),s->palette_tiles);
  Control *c;
  s->hero_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
  gtk_box_append(GTK_BOX(surface), s->hero_box);
  g_signal_connect(s->palette_search,"changed",G_CALLBACK(palette_search_changed),s);
  g_signal_connect(s->profile_picker,"notify::selected",G_CALLBACK(palette_pick_profile),s);
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
  bind_control(s, card, "Ограничить курсор игровой областью", "hotkeys.confine",
               KEY_F1 + 11);
  bind_control(s, card, "Полноэкранный режим", "hotkeys.fullscreen",
               KEY_F1 + 10);
  gtk_box_append(
      GTK_BOX(card),
      wrap_label("Нажмите на назначение, затем нужную клавишу или ПКМ. Escape "
                 "отменяет ввод; Backspace очищает назначение. Системные "
                 "сочетания не передаются игре.",
                 "clbb-muted"));
  card = section(b, "Матч");
  bind_control(s, card, "Открыть / закрыть статистику", "common.bindings.match:statistics.key", 0x01000001);
  card = section(b, "Запись отладки управления");
  switch_control(s, card, "Запись отладки", "debug.recording", 0);
  switch_control(s, card, "Автоматические метки расхождений", "debug.auto_marks", 1);
  bind_control(s, card, "Поставить метку", "debug.mark_shortcut", 0x04000000 | 'S');
  gtk_box_append(GTK_BOX(card), wrap_label("По метке сохраняются 5 секунд до и 5 после: ввод, координаты, камера и результаты применения. Без меток окно хранится только в памяти. По умолчанию Ctrl+S; можно назначить Ctrl, Shift, Alt с клавишей. Автометки отмечают подозрения, а не доказывают ошибку. Активные фрагменты — до 256 МиБ; старые переносятся в archive.", "clbb-muted"));
  s->debug_status = wrap_label("Запись выключена", "clbb-muted");
  gtk_box_append(GTK_BOX(card), s->debug_status);
  accounts_page(s);
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
static void toggle_conflict(GtkButton *button, gpointer user) {
  ClbbSettings *s=user;
  int code=GPOINTER_TO_INT(g_object_get_data(G_OBJECT(button),"clbb-conflict-key"));
  gboolean allowed=GPOINTER_TO_INT(g_object_get_data(G_OBJECT(button),"clbb-conflict-allowed"));
  g_autofree char *setting=g_strdup_printf("bindings.allow_conflict.%d",code);
  stop_capture(s);
  if(write_value(s,setting,allowed?"false":"true"))clbb_settings_refresh(s);
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
  int live=read_int(s,"hero.id",0);
  int hero=s->selected_profile?s->selected_profile:live;
  int spell=read_int(s,"common.battle_spell_id",0);
  g_autofree char *layout=live==hero?read_text(s,"hero.controls",""):g_strdup("");
  g_autofree char *live_name=read_text(s,"hero.name","Герой");
  GtkStringObject *item=gtk_drop_down_get_selected_item(GTK_DROP_DOWN(s->profile_picker));
  const char *profile_name=s->selected_profile&&item?gtk_string_object_get_string(item):live_name;
  if(hero!=s->hero)stop_capture(s);
  if(hero!=s->hero||live!=s->live_profile||spell!=s->active_spell||g_strcmp0(layout,s->hero_layout)
      ||g_strcmp0(profile_name,s->palette_hero_name)){
    s->live_profile=live;
    s->active_spell=spell;
    g_free(s->palette_hero_name);
    s->palette_hero_name=g_strdup(profile_name);
    build_hero(s,hero,layout);
  }
  char *t = read_text(s, "name", "CLBB");
  gtk_label_set_text(GTK_LABEL(s->brand), t);
  g_free(t);
  t = read_text(s, "debug.status", "Запись выключена");
  gtk_label_set_text(GTK_LABEL(s->debug_status), t);
  g_free(t);
  char *conflict_text=read_text(s,"bindings.conflicts","");
  char **conflicts=g_strsplit(conflict_text,"\n",128);g_free(conflict_text);
  g_autofree char *allowed_text=read_text(s,"bindings.allowed_conflicts","");
  g_auto(GStrv) allowed_keys=g_strsplit(allowed_text,"\n",128);
  for (guint i = 0; i < s->controls->len; ++i) {
    Control *c = g_ptr_array_index(s->controls, i);
    if (s->capture == c)
      continue;
    int n = read_int(s, c->key, c->fallback);
    switch (c->kind) {
    case CONTROL_BIND:
      if (!strcmp(c->key, "debug.mark_shortcut")) {
        char *base = key_name(n & ~0x0e000000);
        t = g_strdup_printf("%s%s%s%s", n & 0x04000000 ? "Ctrl+" : "", n & 0x02000000 ? "Shift+" : "", n & 0x08000000 ? "Alt+" : "", base);
        g_free(base);
      } else t = key_name(n);
      gtk_button_set_label(GTK_BUTTON(c->widget), t);
      g_free(t);
      gboolean conflict=FALSE,allowed=FALSE;
      if(s->hero==s->live_profile&&s->live_profile>0)
        for(guint k=0;conflicts[k];++k)if(n!=0&&atoi(conflicts[k])==n){conflict=TRUE;break;}
      if(conflict)for(guint k=0;allowed_keys[k];++k)
        if(atoi(allowed_keys[k])==n){allowed=TRUE;break;}
      if(conflict&&!allowed)gtk_widget_add_css_class(c->widget,"clbb-conflict");
      else gtk_widget_remove_css_class(c->widget,"clbb-conflict");
      if(conflict&&!c->conflict_button){
        c->conflict_button=gtk_button_new();
        gtk_widget_add_css_class(c->conflict_button,"clbb-secondary");
        gtk_box_append(GTK_BOX(c->row),c->conflict_button);
        g_signal_connect(c->conflict_button,"clicked",G_CALLBACK(toggle_conflict),s);
      }
      if(c->conflict_button){
        gtk_widget_set_visible(c->conflict_button,conflict);
        g_object_set_data(G_OBJECT(c->conflict_button),"clbb-conflict-key",GINT_TO_POINTER(n));
        g_object_set_data(G_OBJECT(c->conflict_button),"clbb-conflict-allowed",GINT_TO_POINTER(allowed));
        gtk_button_set_label(GTK_BUTTON(c->conflict_button),allowed?"✓ Конфликт разрешён":"Разрешить конфликт");
        gtk_widget_set_tooltip_text(c->conflict_button,allowed?"Нажмите, чтобы вернуть блокировку":"Если действия доступны одновременно, сработает одно");
      }
      gtk_widget_set_tooltip_text(c->widget,conflict&&!allowed?"Конфликт назначений: клавиша отключена":NULL);
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
        int spell_id=read_int(s,"common.battle_spell_id",0);
        gboolean plain = spell_id==20080 || spell_id==20030 || spell_id==20190;
        if (plain != c->plain_spell_mode) {
          const char *const plain_modes[] = {"При отпускании", "Моментальное нажатие", NULL};
          const char *const aimed_modes[] = {"Моментальное прожатие", "Обычный навык", "Моментальный с наведением", "Прожатие с удержанием", NULL};
          GtkStringList *model = gtk_string_list_new(plain ? plain_modes : aimed_modes);
          gtk_drop_down_set_model(GTK_DROP_DOWN(c->widget), G_LIST_MODEL(model));
          g_object_unref(model);
          gboolean personal = g_str_has_prefix(c->key, "hero.");
          g_free(c->key);
          c->key = plain ? g_strdup_printf("%s.bindings.extra:2:%d.cast_mode",personal?"hero":"common",spell_id)
                         :g_strdup_printf("%s.bindings.extra:2.cast_mode",personal?"hero":"common");
          c->plain_spell_mode = plain;
        }
        n = read_int(s, c->key, c->fallback);
      }
      if (c->battle_spell_mode && !c->plain_spell_mode) {
        static const guint spell_indices[] = {1, 2, 0, 3};
        gtk_drop_down_set_selected(GTK_DROP_DOWN(c->widget), spell_indices[CLAMP(n, 0, 3)]);
      } else {
        gtk_drop_down_set_selected(GTK_DROP_DOWN(c->widget), CLAMP(n, 0, g_str_has_suffix(c->key,"movement.click_mode")?2:1));
      }
      break;
    case CONTROL_VOLUME:
      gtk_range_set_value(GTK_RANGE(c->widget), CLAMP(n, 0, 100));
      break;
    }
    if (c->visible_if) gtk_widget_set_visible(c->row,(read_int(s,c->visible_if,0)==c->visible_mode)!=c->visible_not);
    if (c->inherit_key) {
      gboolean own=read_bool(s,c->inherit_key,FALSE);
      gtk_widget_set_sensitive(c->inherit_reset,own);
      if(c->inherit_status){
        gtk_label_set_text(GTK_LABEL(c->inherit_status),own?"Личное значение":"Наследует общее");
        gtk_widget_set_visible(c->inherit_reset,own);
      }
      gtk_button_set_icon_name(GTK_BUTTON(c->inherit_reset),own?"edit-undo-symbolic":"insert-link-symbolic");
      gtk_widget_set_tooltip_text(c->inherit_reset,own?"Вернуть общее значение":"Как у всех героев");
      if (own) gtk_widget_remove_css_class(c->widget,"clbb-inherited");
      else gtk_widget_add_css_class(c->widget,"clbb-inherited");
    }

  }
  g_strfreev(conflicts);
  for(GtkWidget *row=gtk_widget_get_first_child(s->palette_tiles);row;row=gtk_widget_get_next_sibling(row)){
    GtkWidget *tile=gtk_flow_box_child_get_child(GTK_FLOW_BOX_CHILD(row));
    const char *slot=g_object_get_data(G_OBJECT(tile),"clbb-slot");
    for(GtkWidget *pane=gtk_widget_get_first_child(s->hero_box);pane;pane=gtk_widget_get_next_sibling(pane)){
      if(g_strcmp0(slot,g_object_get_data(G_OBJECT(pane),"clbb-slot")))continue;
      const char *title=g_object_get_data(G_OBJECT(pane),"clbb-title");
      const char *bind=g_object_get_data(G_OBJECT(pane),"clbb-key");
      int fallback=GPOINTER_TO_INT(g_object_get_data(G_OBJECT(pane),"clbb-fallback"));
      g_autofree char *key=key_name(read_int(s,bind,fallback));
      g_autofree char *caption=g_strdup_printf("%s · %s",title,key);
      if(g_strcmp0(caption,gtk_button_get_label(GTK_BUTTON(tile))))
        gtk_button_set_label(GTK_BUTTON(tile),caption);
      break;
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
  g_object_set_data(G_OBJECT(s->panel),"clbb-accounts-owner",NULL);
  g_ptr_array_unref(s->account_ids);
  g_ptr_array_unref(s->account_labels);
  g_array_unref(s->account_flags);
  g_free(s->hero_layout);
  g_free(s->palette_selected);
  g_free(s->palette_hero_name);
  g_free(s->offline_forms);
  g_array_unref(s->profile_ids);
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
