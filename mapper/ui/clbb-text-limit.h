/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include <gtk/gtk.h>
#include <string.h>

/* Android LengthFilter and the Unity input API count UTF-16 code units. GTK
 * stores UTF-8 and its built-in max-length counts Unicode code points. */
static inline int clbb_text_units(const char *text)
{
    int units = 0;
    for (; *text; text = g_utf8_next_char(text))
        units += g_utf8_get_char(text) >= 0x10000 ? 2 : 1;
    return units;
}

static inline gsize clbb_text_prefix(const char *text, gsize bytes, int units)
{
    const char *cursor = text;
    while ((gsize)(cursor - text) < bytes) {
        gunichar ch = g_utf8_get_char_validated(cursor, bytes - (cursor - text));
        if (ch == (gunichar)-1 || ch == (gunichar)-2)
            break;
        int width = ch >= 0x10000 ? 2 : 1;
        if (units < width)
            break;
        units -= width;
        cursor = g_utf8_next_char(cursor);
    }
    return cursor - text;
}

static inline GtkEditable *clbb_text_delegate(GtkEditable *editable)
{
    GtkEditable *delegate = gtk_editable_get_delegate(editable);
    return delegate ? delegate : editable;
}

static inline int clbb_text_limit(GtkEditable *editable)
{
    editable = clbb_text_delegate(editable);
    guint encoded = GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(editable), "clbb-text-limit"));
    return encoded ? (int)(encoded - 1) : -1;
}

static void clbb_filter_insertion(GtkEditable *editable, const char *text,
                                 int length, int *position, gpointer unused)
{
    (void)unused;
    int limit = clbb_text_limit(editable);
    if (limit < 0)
        return;
    gsize bytes = length < 0 ? strlen(text) : (gsize)length;
    gsize keep = clbb_text_prefix(text, bytes, limit - clbb_text_units(gtk_editable_get_text(editable)));
    if (keep == bytes)
        return;
    g_signal_stop_emission_by_name(editable, "insert-text");
    if (keep)
        gtk_editable_insert_text(editable, text, keep, position);
}

static inline void clbb_text_set_limit(GtkEditable *editable, int limit)
{
    if (limit == clbb_text_limit(editable))
        return;
    editable = clbb_text_delegate(editable);
    GObject *object = G_OBJECT(editable);
    if (!g_object_get_data(object, "clbb-text-limit-connected")) {
        g_signal_connect(editable, "insert-text", G_CALLBACK(clbb_filter_insertion), NULL);
        g_object_set_data(object, "clbb-text-limit-connected", GINT_TO_POINTER(1));
    }
    g_object_set_data(object, "clbb-text-limit", limit < 0 ? NULL : GUINT_TO_POINTER((guint)limit + 1));
    if (limit < 0)
        return;
    const char *text = gtk_editable_get_text(editable);
    gsize bytes = strlen(text);
    gsize keep = clbb_text_prefix(text, bytes, limit);
    if (keep != bytes)
        gtk_editable_delete_text(editable, g_utf8_pointer_to_offset(text, text + keep), -1);
}
