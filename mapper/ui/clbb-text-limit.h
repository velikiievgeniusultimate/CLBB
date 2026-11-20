/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include <gtk/gtk.h>
#include <string.h>

typedef struct {
    int utf16;
    int bytes;
} ClbbTextLimits;

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

static inline ClbbTextLimits clbb_text_limits(GtkEditable *editable)
{
    const ClbbTextLimits *limits = g_object_get_data(G_OBJECT(clbb_text_delegate(editable)), "clbb-text-limits");
    return limits ? *limits : (ClbbTextLimits){-1, -1};
}

/* The game uses UTF-8 bytes for the message. Android LengthFilter imposes
 * an independent UTF-16 limit on the original editor. */
static inline gsize clbb_text_fit(const char *text, gsize bytes, ClbbTextLimits remaining)
{
    if (remaining.bytes >= 0)
        bytes = MIN(bytes, (gsize)remaining.bytes);
    int units = remaining.utf16 < 0 ? G_MAXINT : remaining.utf16;
    return clbb_text_prefix(text, bytes, units);
}

static void clbb_filter_insertion(GtkEditable *editable, const char *text,
                                 int length, int *position, gpointer data)
{
    ClbbTextLimits remaining = *(const ClbbTextLimits *)data;
    if (remaining.bytes < 0 && remaining.utf16 < 0)
        return;
    const char *present = gtk_editable_get_text(editable);
    if (remaining.bytes >= 0)
        remaining.bytes = MAX(0, remaining.bytes - (int)strlen(present));
    if (remaining.utf16 >= 0)
        remaining.utf16 = MAX(0, remaining.utf16 - clbb_text_units(present));
    gsize bytes = length < 0 ? strlen(text) : (gsize)length;
    gsize keep = clbb_text_fit(text, bytes, remaining);
    if (keep == bytes)
        return;
    g_signal_stop_emission_by_name(editable, "insert-text");
    if (keep)
        gtk_editable_insert_text(editable, text, keep, position);
}

static inline void clbb_text_set_limits(GtkEditable *editable, ClbbTextLimits next)
{
    editable = clbb_text_delegate(editable);
    GObject *object = G_OBJECT(editable);
    ClbbTextLimits *limits = g_object_get_data(object, "clbb-text-limits");
    if (!limits) {
        if (next.utf16 < 0 && next.bytes < 0)
            return;
        limits = g_new(ClbbTextLimits, 1);
        g_object_set_data_full(object, "clbb-text-limits", limits, g_free);
        g_signal_connect(editable, "insert-text", G_CALLBACK(clbb_filter_insertion), limits);
    } else if (limits->utf16 == next.utf16 && limits->bytes == next.bytes) {
        return;
    }
    *limits = next;
    const char *text = gtk_editable_get_text(editable);
    gsize bytes = strlen(text);
    gsize keep = clbb_text_fit(text, bytes, next);
    if (keep != bytes)
        gtk_editable_delete_text(editable, g_utf8_pointer_to_offset(text, text + keep), -1);
}
