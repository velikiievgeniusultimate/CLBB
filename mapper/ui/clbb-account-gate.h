/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include <gtk/gtk.h>

typedef struct ClbbAccountGate ClbbAccountGate;
typedef const char *(*ClbbChooserRead)(void *, const char *);
typedef gboolean (*ClbbChooserTap)(void *, double, double);

ClbbAccountGate *clbb_account_gate_new(GtkOverlay *overlay, GtkWidget *viewport,
                                      ClbbChooserRead read, ClbbChooserTap tap, void *user);
void clbb_account_gate_refresh(ClbbAccountGate *gate);
