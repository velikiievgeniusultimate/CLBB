/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "clbb-account-gate.h"
#include "clbb-viewport.h"
#include <math.h>
#include <stdio.h>
#include <string.h>

struct ClbbAccountGate {
    GtkWidget *card, *viewport, *code, *back;
    GtkStack *pages;
    GtkOverlay *overlay;
    ClbbChooserRead read;
    ClbbChooserTap tap;
    void *user;
    guint64 generation;
    guint bounds[4];
    gboolean revealed;
};

typedef struct {
    guint64 generation;
    guint left, top, right, bottom, x, y;
} Layout;

static gboolean read_layout(ClbbAccountGate *gate, Layout *l) {
    const char *s = gate->read(gate->user, "account.chooser");
    return s && sscanf(s, "%" G_GUINT64_FORMAT " %u %u %u %u %u %u", &l->generation,
        &l->left, &l->top, &l->right, &l->bottom, &l->x, &l->y) == 7 &&
        l->generation && l->left < l->right && l->right <= 1000000 &&
        l->top < l->bottom && l->bottom <= 1000000;
}
static gboolean can_tap(const Layout *l) {
    return l->left < l->x && l->x < l->right && l->top < l->y && l->y < l->bottom;
}
static void open_code(GtkButton *button, ClbbAccountGate *gate) {
    (void)button;
    Layout l;
    if (read_layout(gate, &l) && l.generation == gate->generation && can_tap(&l))
        gate->tap(gate->user, l.x / 1000000., l.y / 1000000.);
}
static void warn(GtkButton *button, ClbbAccountGate *gate) {
    (void)button;
    gtk_stack_set_visible_child_name(gate->pages, "warning");
    gtk_widget_grab_focus(gate->back);
}
static void back(GtkButton *button, ClbbAccountGate *gate) {
    (void)button;
    gtk_stack_set_visible_child_name(gate->pages, "code");
    gtk_widget_grab_focus(gate->code);
}
static void reveal(GtkButton *button, ClbbAccountGate *gate) {
    (void)button;
    gate->revealed = TRUE;
    gtk_widget_set_visible(gate->card, FALSE);
}
static gboolean position(GtkOverlay *overlay, GtkWidget *child, GdkRectangle *out, ClbbAccountGate *gate) {
    (void)overlay;
    graphene_rect_t v;
    if (child != gate->card || !clbb_viewport_get_content_rect(gate->viewport, &v)) return FALSE;
    double sx = v.size.width / 1000000., sy = v.size.height / 1000000.;
    out->x = floor(v.origin.x + gate->bounds[0]*sx);
    out->y = floor(v.origin.y + gate->bounds[1]*sy);
    out->width = ceil(v.origin.x + gate->bounds[2]*sx) - out->x;
    out->height = ceil(v.origin.y + gate->bounds[3]*sy) - out->y;
    return TRUE;
}
static GtkWidget *page(const char *title, const char *detail) {
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 6);
    gtk_widget_set_halign(box, GTK_ALIGN_CENTER);
    gtk_widget_set_valign(box, GTK_ALIGN_CENTER);
    GtkWidget *heading = gtk_label_new(title), *text = gtk_label_new(detail);
    gtk_widget_add_css_class(heading, "heading");
    gtk_label_set_wrap(GTK_LABEL(heading), TRUE);
    gtk_label_set_wrap(GTK_LABEL(text), TRUE);
    gtk_label_set_justify(GTK_LABEL(heading), GTK_JUSTIFY_CENTER);
    gtk_label_set_justify(GTK_LABEL(text), GTK_JUSTIFY_CENTER);
    gtk_label_set_max_width_chars(GTK_LABEL(text), 43);
    gtk_box_append(GTK_BOX(box), heading);
    gtk_box_append(GTK_BOX(box), text);
    return box;
}
static GtkWidget *button(GtkWidget *box, const char *text, const char *style, GCallback fn, ClbbAccountGate *gate) {
    GtkWidget *b = gtk_button_new_with_label(text);
    gtk_widget_add_css_class(b, style);
    g_signal_connect(b, "clicked", fn, gate);
    gtk_box_append(GTK_BOX(box), b);
    return b;
}
ClbbAccountGate *clbb_account_gate_new(GtkOverlay *overlay, GtkWidget *viewport,
                                      ClbbChooserRead read, ClbbChooserTap tap, void *user) {
    ClbbAccountGate *gate = g_new0(ClbbAccountGate, 1);
    gate->viewport = viewport;
    gate->overlay = overlay;
    gate->read = read;
    gate->tap = tap;
    gate->user = user;
    gate->card = gtk_stack_new();
    gate->pages = GTK_STACK(gate->card);
    gtk_widget_add_css_class(gate->card, "clbb-account-gate");
    GtkWidget *code = page("Вход в аккаунт", "MOONTON пришлёт код на вашу почту.");
    gate->code = button(code, "Войти по коду MOONTON", "primary", G_CALLBACK(open_code), gate);
    button(code, "Другие способы входа", "secondary", G_CALLBACK(warn), gate);
    GtkWidget *warning = page("Неподдерживаемые способы входа", "Эти способы могут не сработать в CLBB. Для входа используйте код из письма.");
    gate->back = button(warning, "Вернуться к входу по коду", "primary", G_CALLBACK(back), gate);
    button(warning, "Всё равно показать", "secondary", G_CALLBACK(reveal), gate);
    gtk_stack_add_named(gate->pages, code, "code");
    gtk_stack_add_named(gate->pages, warning, "warning");
    gtk_widget_set_visible(gate->card, FALSE);
    gtk_overlay_add_overlay(overlay, gate->card);
    g_signal_connect(overlay, "get-child-position", G_CALLBACK(position), gate);
    g_object_set_data_full(G_OBJECT(overlay), "clbb-account-gate", gate, g_free);
    GtkCssProvider *style = gtk_css_provider_new();
    gtk_css_provider_load_from_string(style,
        ".clbb-account-gate { background:#211b23; border:1px solid #674454; border-radius:8px; padding:6px; }"
        ".clbb-account-gate label { color:#e4d7df; } .clbb-account-gate .heading { color:#fff5fa; font-weight:bold; font-size:1.13em; }"
        ".clbb-account-gate button { color:#fff3f8; background:#30252e; border:1px solid #694955; border-radius:6px; }"
        ".clbb-account-gate button.primary { background:#77364e; border-color:#cd6086; }"
        ".clbb-account-gate button:hover, .clbb-account-gate button:focus-visible { background:#633549; border-color:#fc87ae; }");
    gtk_style_context_add_provider_for_display(gtk_widget_get_display(GTK_WIDGET(overlay)), GTK_STYLE_PROVIDER(style),
                                               GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
    g_object_unref(style);
    return gate;
}
void clbb_account_gate_refresh(ClbbAccountGate *gate) {
    Layout l;
    if (!read_layout(gate, &l)) {
        gtk_widget_set_visible(gate->card, FALSE);
        return;
    }
    if (l.generation != gate->generation) {
        gate->generation = l.generation;
        gate->revealed = FALSE;
        gtk_stack_set_visible_child_name(gate->pages, "code");
    }
    if (gate->revealed) return;
    guint bounds[] = {l.left,l.top,l.right,l.bottom};
    if (memcmp(bounds, gate->bounds, sizeof(bounds)) != 0) {
        memcpy(gate->bounds, bounds, sizeof(bounds));
        gtk_widget_queue_allocate(GTK_WIDGET(gate->overlay));
    }
    gtk_widget_set_sensitive(gate->code, can_tap(&l));
    gtk_widget_set_visible(gate->card, TRUE);
}
