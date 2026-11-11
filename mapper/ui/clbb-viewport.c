/* CLBB-owned fixed Android logical viewport. GTK carries the transform through
 * rendering, picking and compute_point; input does not need a second mapping. */
#include "clbb-viewport.h"

struct _ClbbViewport {
    GtkWidget parent_instance;
    GtkWidget *child;
    int logical_width;
    int logical_height;
};
G_DEFINE_FINAL_TYPE(ClbbViewport, clbb_viewport, GTK_TYPE_WIDGET)

static gboolean content_rect(ClbbViewport *self, int width, int height,
                             graphene_rect_t *rect, float *scale)
{
    if (width <= 0 || height <= 0) {
        graphene_rect_init(rect, 0.f, 0.f, 0.f, 0.f);
        if (scale) *scale = 0.f;
        return FALSE;
    }
    const double factor = MIN((double)width / self->logical_width,
                              (double)height / self->logical_height);
    const double w = self->logical_width * factor;
    const double h = self->logical_height * factor;
    graphene_rect_init(rect, (width - w) / 2., (height - h) / 2., w, h);
    if (scale) *scale = factor;
    return TRUE;
}

static void clbb_viewport_measure(GtkWidget *widget, GtkOrientation orientation,
                                  int for_size, int *minimum, int *natural,
                                  int *minimum_baseline, int *natural_baseline)
{
    (void)widget;
    (void)for_size;
    /* Never propagate Android's 4000px requests into the desktop window. */
    *minimum = orientation == GTK_ORIENTATION_HORIZONTAL ? 320 : 240;
    *natural = orientation == GTK_ORIENTATION_HORIZONTAL ? 960 : 540;
    if (minimum_baseline) *minimum_baseline = -1;
    if (natural_baseline) *natural_baseline = -1;
}

static void clbb_viewport_size_allocate(GtkWidget *widget, int width, int height,
                                        int baseline)
{
    ClbbViewport *self = CLBB_VIEWPORT(widget);
    (void)baseline;
    if (!self->child || !gtk_widget_should_layout(self->child)) return;
    graphene_rect_t rect;
    float scale;
    if (!content_rect(self, width, height, &rect, &scale)) return;
    GskTransform *transform = gsk_transform_translate(NULL, &rect.origin);
    transform = gsk_transform_scale(transform, scale, scale);
    /* gtk_widget_allocate takes ownership of the transform. */
    gtk_widget_allocate(self->child, self->logical_width,
                        self->logical_height, -1, transform);
}

static void clbb_viewport_snapshot(GtkWidget *widget, GtkSnapshot *snapshot)
{
    ClbbViewport *self = CLBB_VIEWPORT(widget);
    const GdkRGBA black = {0.f, 0.f, 0.f, 1.f};
    graphene_rect_t bounds;
    graphene_rect_init(&bounds, 0.f, 0.f, gtk_widget_get_width(widget),
                       gtk_widget_get_height(widget));
    gtk_snapshot_append_color(snapshot, &black, &bounds);
    if (self->child && gtk_widget_get_visible(self->child))
        gtk_widget_snapshot_child(widget, self->child, snapshot);
}

static void clbb_viewport_dispose(GObject *object)
{
    ClbbViewport *self = CLBB_VIEWPORT(object);
    if (self->child) {
        gtk_widget_unparent(self->child);
        self->child = NULL;
    }
    G_OBJECT_CLASS(clbb_viewport_parent_class)->dispose(object);
}

static void clbb_viewport_class_init(ClbbViewportClass *klass)
{
    GtkWidgetClass *widget = GTK_WIDGET_CLASS(klass);
    G_OBJECT_CLASS(klass)->dispose = clbb_viewport_dispose;
    widget->measure = clbb_viewport_measure;
    widget->size_allocate = clbb_viewport_size_allocate;
    widget->snapshot = clbb_viewport_snapshot;
    gtk_widget_class_set_css_name(widget, "clbb-viewport");
    gtk_widget_class_set_accessible_role(widget, GTK_ACCESSIBLE_ROLE_GROUP);
}

static void clbb_viewport_init(ClbbViewport *self)
{
    self->logical_width = 1280;
    self->logical_height = 720;
    gtk_widget_set_overflow(GTK_WIDGET(self), GTK_OVERFLOW_HIDDEN);
    gtk_widget_set_hexpand(GTK_WIDGET(self), TRUE);
    gtk_widget_set_vexpand(GTK_WIDGET(self), TRUE);
    gtk_widget_add_css_class(GTK_WIDGET(self), "clbb-viewport");
}

GtkWidget *clbb_viewport_new(GtkWidget *child, int width, int height)
{
    g_return_val_if_fail(GTK_IS_WIDGET(child), NULL);
    g_return_val_if_fail(gtk_widget_get_parent(child) == NULL, NULL);
    g_return_val_if_fail(width > 0 && width <= 16384 && height > 0 && height <= 16384, NULL);
    ClbbViewport *self = g_object_new(CLBB_TYPE_VIEWPORT, NULL);
    self->logical_width = width;
    self->logical_height = height;
    self->child = child;
    gtk_widget_set_parent(child, GTK_WIDGET(self));
    return GTK_WIDGET(self);
}

void clbb_viewport_set_resolution(GtkWidget *widget, int width, int height)
{
    g_return_if_fail(CLBB_IS_VIEWPORT(widget));
    g_return_if_fail(width > 0 && width <= 16384 && height > 0 && height <= 16384);
    ClbbViewport *self = CLBB_VIEWPORT(widget);
    if (self->logical_width == width && self->logical_height == height) return;
    self->logical_width = width;
    self->logical_height = height;
    gtk_widget_queue_allocate(widget);
    gtk_widget_queue_draw(widget);
}

void clbb_viewport_get_resolution(GtkWidget *widget, int *width, int *height)
{
    g_return_if_fail(CLBB_IS_VIEWPORT(widget));
    ClbbViewport *self = CLBB_VIEWPORT(widget);
    if (width) *width = self->logical_width;
    if (height) *height = self->logical_height;
}

gboolean clbb_viewport_get_content_rect(GtkWidget *widget, graphene_rect_t *rect)
{
    g_return_val_if_fail(CLBB_IS_VIEWPORT(widget), FALSE);
    g_return_val_if_fail(rect != NULL, FALSE);
    return content_rect(CLBB_VIEWPORT(widget), gtk_widget_get_width(widget),
                        gtk_widget_get_height(widget), rect, NULL);
}
