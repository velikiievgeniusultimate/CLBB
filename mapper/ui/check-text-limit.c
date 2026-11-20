/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "clbb-text-limit.h"

static void check_editor(GtkEditable *entry)
{
    GtkEditable *input = gtk_editable_get_delegate(entry);
    if (!input)
        input = entry;
    clbb_text_set_limits(entry, (ClbbTextLimits){6, -1});
    int at = 0;
    gtk_editable_insert_text(input, "абвгдеёж", -1, &at);
    g_assert_cmpstr(gtk_editable_get_text(entry), ==, "абвгде");
    g_assert_cmpint(at, ==, 6);

    gtk_editable_delete_text(input, 2, 6);
    at = 2;
    gtk_editable_insert_text(input, "🌙🌞🌟", -1, &at);
    g_assert_cmpstr(gtk_editable_get_text(entry), ==, "аб🌙🌞");
    g_assert_cmpint(at, ==, 4);

    clbb_text_set_limits(entry, (ClbbTextLimits){5, -1});
    g_assert_cmpstr(gtk_editable_get_text(entry), ==, "аб🌙");
    gtk_editable_set_text(entry, "1234🌙5");
    g_assert_cmpstr(gtk_editable_get_text(entry), ==, "1234");
    clbb_text_set_limits(entry, (ClbbTextLimits){0, -1});
    gtk_editable_set_text(entry, "з");
    g_assert_cmpstr(gtk_editable_get_text(entry), ==, "");
    clbb_text_set_limits(entry, (ClbbTextLimits){-1, -1});
    gtk_editable_set_text(entry, "абвгдеёж");
    g_assert_cmpint(clbb_text_units(gtk_editable_get_text(entry)), ==, 8);

    gtk_editable_set_text(entry, "");
    clbb_text_set_limits(entry, (ClbbTextLimits){-1, 6});
    at = 0;
    gtk_editable_insert_text(input, "абвг", -1, &at);
    g_assert_cmpstr(gtk_editable_get_text(entry), ==, "абв");
    gtk_editable_set_text(entry, "長度限制");
    g_assert_cmpstr(gtk_editable_get_text(entry), ==, "長度");
    gtk_editable_set_text(entry, "я🌙а");
    g_assert_cmpstr(gtk_editable_get_text(entry), ==, "я🌙");
    gtk_editable_set_text(entry, "abc🌙");
    g_assert_cmpstr(gtk_editable_get_text(entry), ==, "abc");
    clbb_text_set_limits(entry, (ClbbTextLimits){2, 6});
    gtk_editable_set_text(entry, "а🌙б");
    g_assert_cmpstr(gtk_editable_get_text(entry), ==, "а");
}

int main(void)
{
    gtk_init();
    GtkWidget *entry = g_object_ref_sink(gtk_entry_new());
    GtkWidget *text = g_object_ref_sink(gtk_text_new());
    check_editor(GTK_EDITABLE(entry));
    check_editor(GTK_EDITABLE(text));
    g_object_unref(entry);
    g_object_unref(text);
    g_print("CHAT_LENGTH_OK native typing, paste, replacement, dynamic UTF-8 and UTF-16 limits\n");
}
