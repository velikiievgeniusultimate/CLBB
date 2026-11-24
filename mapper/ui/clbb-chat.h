/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include <gtk/gtk.h>

#define CLBB_CHAT_DEFAULT_LIMIT 60
#define CLBB_CHAT_BATTLE_SITE 1

typedef struct {
    guint64 owner;
    guint site;
    int channel;
    guint editing, x, y;
    int limit;
    guint ready; /* 0: unknown, 1: busy, 2: ready for another message */
} ClbbChatTarget;

gboolean atl_chat_begin(GtkWindow *window, const ClbbChatTarget *target);
gboolean atl_chat_active(GtkWindow *window);
gboolean atl_chat_waiting(GtkWindow *window);
void atl_chat_abort_pending(GtkWindow *window);
void atl_chat_sync_limit(GtkWindow *window, const ClbbChatTarget *target);
gboolean atl_chat_batch_pending(GtkWindow *window);
void atl_chat_cancel_batch(GtkWindow *window);
gboolean atl_chat_begin_followup(GtkWindow *window, const ClbbChatTarget *target, gint64 now);
gboolean clbb_chat_site(GtkWindow *window, ClbbChatTarget *target, gboolean live);
