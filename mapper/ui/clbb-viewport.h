#pragma once
#include <gtk/gtk.h>

G_BEGIN_DECLS

#define CLBB_TYPE_VIEWPORT (clbb_viewport_get_type())
G_DECLARE_FINAL_TYPE(ClbbViewport, clbb_viewport, CLBB, VIEWPORT, GtkWidget)

/* Owns child through ordinary GTK parenting. The Android child always receives
 * this logical allocation; only its presentation scales to the host window. */
GtkWidget *clbb_viewport_new(GtkWidget *child, int width, int height);
void clbb_viewport_set_resolution(GtkWidget *widget, int width, int height);
void clbb_viewport_get_resolution(GtkWidget *widget, int *width, int *height);
/* Rectangle in viewport-local physical coordinates, including letterbox offset.
 * Returns FALSE until the viewport has a nonzero allocation. */
gboolean clbb_viewport_get_content_rect(GtkWidget *widget, graphene_rect_t *rect);

/* Inward-rounded content bounds in native surface device pixels, excluding CSD. */
gboolean clbb_viewport_get_surface_content_rect(GtkWidget *widget, GdkRectangle *rect);

G_END_DECLS
