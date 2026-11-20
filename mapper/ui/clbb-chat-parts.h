/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include "clbb-text-limit.h"

/* A nonempty, sendable draft returns one or two byte ranges into the input. */
typedef struct {
    const char *first, *second;
    gsize first_bytes, second_bytes;
} ClbbChatParts;

static inline ClbbChatParts clbb_chat_parts(const char *text, ClbbTextLimits limits)
{
    while (*text && g_unichar_isspace(g_utf8_get_char(text)))
        text = g_utf8_next_char(text);
    const char *end = text + strlen(text);
    while (end > text) {
        const char *last = g_utf8_prev_char(end);
        if (!g_unichar_isspace(g_utf8_get_char(last)))
            break;
        end = last;
    }
    if (end == text)
        return (ClbbChatParts){0};
    gsize length = end - text;
    gsize fit = clbb_text_fit(text, length, limits);
    if (fit == length)
        return (ClbbChatParts){text, NULL, length, 0};

    const char *split = NULL, *next = NULL;
    for (const char *p = text; p <= text + fit;) {
        if (!g_unichar_isspace(g_utf8_get_char(p))) {
            p = g_utf8_next_char(p);
            continue;
        }
        split = p;
        do {
            p = g_utf8_next_char(p);
        } while (p < end && g_unichar_isspace(g_utf8_get_char(p)));
        next = p;
    }
    if (!split || clbb_text_fit(next, end - next, limits) != (gsize)(end - next))
        return (ClbbChatParts){0};
    return (ClbbChatParts){text, next, split - text, end - next};
}
