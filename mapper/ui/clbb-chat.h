/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include <gtk/gtk.h>

#define CLBB_CHAT_DEFAULT_LIMIT 60

typedef struct {
    guint64 owner;
    guint site;
    int channel;
    guint editing, x, y;
    int limit;
} ClbbChatTarget;

gboolean atl_chat_begin(GtkWindow *window, const ClbbChatTarget *target);
gboolean atl_chat_active(GtkWindow *window);
gboolean atl_chat_waiting(GtkWindow *window);
void atl_chat_abort_pending(GtkWindow *window);
void atl_chat_sync_limit(GtkWindow *window, const ClbbChatTarget *target);
gboolean clbb_chat_site(GtkWindow *window, ClbbChatTarget *target);
