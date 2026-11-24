#pragma once
#include "clbb-settings.h"

typedef struct ClbbEditor ClbbEditor;
typedef void (*ClbbEditorDetails)(ClbbSettings *, GtkWidget *canvas, const GdkRectangle *anchor, const char *slot);
ClbbEditor *clbb_editor_new(GtkOverlay *overlay, const ClbbSettingsCallbacks *callbacks,
                            ClbbSettings *owner, ClbbEditorDetails details);
void clbb_editor_show(ClbbEditor *);
gboolean clbb_editor_creation_visible(ClbbEditor *);
void clbb_editor_close_creation(ClbbEditor *);
void clbb_editor_hide(ClbbEditor *);
gboolean clbb_editor_visible(ClbbEditor *);
gboolean clbb_editor_input(ClbbEditor *, int code, gboolean down, gboolean repeat);
void clbb_settings_close_editor_menu(ClbbSettings *);
void clbb_editor_free(ClbbEditor *);
char *clbb_binding_label(int code);
char *clbb_editor_slot_title(const char *slot);
