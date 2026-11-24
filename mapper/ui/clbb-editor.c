#include "clbb-editor.h"
#include "clbb-viewport.h"
#include <math.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char *slot, *path, *label, *title;
    double u, v;
    gboolean conflict;
    GdkRectangle box;
} Marker;

struct ClbbEditor {
    ClbbSettings *owner;
    ClbbSettingsCallbacks cb;
    ClbbEditorDetails details;
    GtkWidget *layer, *canvas, *viewport, *gear, *heading, *hint, *popup;
    GPtrArray *markers;
    char *snapshot, *selected, *capture;
    guint timer;
    double add_u, add_v;
};

static const char help[] = "Дважды ЛКМ — назначить клавишу     ·     ПКМ по карте — добавить точку";

static void free_marker(gpointer data) {
    Marker *m = data;
    g_free(m->slot); g_free(m->path); g_free(m->label); g_free(m->title); g_free(m);
}

static Marker *selected(ClbbEditor *e) {
    for (guint i = 0; i < e->markers->len; i++) {
        Marker *m = g_ptr_array_index(e->markers, i);
        if (!g_strcmp0(m->path, e->selected)) return m;
    }
    return NULL;
}

static void hint(ClbbEditor *e, const char *text) {
    gtk_label_set_text(GTK_LABEL(e->hint), text);
}

static void update_selection(ClbbEditor *e) {
    Marker *m = selected(e);
    gtk_label_set_text(GTK_LABEL(e->heading), m ? m->title : "Карта кнопок");
    gtk_widget_set_sensitive(e->gear, m != NULL);
    gtk_widget_queue_draw(e->canvas);
}

static gboolean refresh(gpointer data) {
    ClbbEditor *e = data;
    const char *rows = e->cb.read(e->cb.user, "editor.layout");
    if (!g_strcmp0(rows, e->snapshot)) return G_SOURCE_CONTINUE;
    g_free(e->snapshot); e->snapshot = g_strdup(rows ? rows : "");
    g_ptr_array_set_size(e->markers, 0);
    g_auto(GStrv) lines = g_strsplit(e->snapshot, "\n", -1);
    for (guint i = 0; lines[i]; i++) {
        g_auto(GStrv) col = g_strsplit(lines[i], "\t", 7);
        if (g_strv_length(col) != 6) continue;
        Marker *m = g_new0(Marker, 1);
        m->slot = g_strdup(col[0]); m->path = g_strdup(col[1]);
        m->label = atoi(col[2]) ? clbb_binding_label(atoi(col[2])) : g_strdup("＋");
        m->u = g_ascii_strtod(col[3], NULL); m->v = g_ascii_strtod(col[4], NULL);
        m->title = clbb_editor_slot_title(m->slot); m->conflict = atoi(col[5]);
        g_ptr_array_add(e->markers, m);
    }
    if (!selected(e)) {
        g_clear_pointer(&e->selected, g_free);
        g_clear_pointer(&e->capture, g_free);
        hint(e, help);
    }
    update_selection(e);
    return G_SOURCE_CONTINUE;
}

static gboolean bounds(ClbbEditor *e, graphene_rect_t *r) {
    if (CLBB_IS_VIEWPORT(e->viewport))
        return clbb_viewport_get_content_rect(e->viewport, r);
    // Standalone settings preview has no Android viewport.
    graphene_rect_init(r, 0, 0, gtk_widget_get_width(e->canvas), gtk_widget_get_height(e->canvas));
    return r->size.width > 0 && r->size.height > 0;
}

static void pill(cairo_t *cr, GdkRectangle r) {
    double radius = r.height / 2.;
    cairo_new_sub_path(cr);
    cairo_arc(cr, r.x+r.width-radius, r.y+radius, radius, -G_PI_2, G_PI_2);
    cairo_arc(cr, r.x+radius, r.y+radius, radius, G_PI_2, 3*G_PI_2);
    cairo_close_path(cr);
}

static void paint(GtkDrawingArea *area, cairo_t *cr, int w, int h, gpointer data) {
    (void)area; (void)w; (void)h;
    ClbbEditor *e = data;
    graphene_rect_t r;
    if (!bounds(e, &r)) return;
    double left=r.origin.x, top=r.origin.y, right=left+r.size.width, bottom=top+r.size.height;
    cairo_rectangle(cr,left,top,r.size.width,r.size.height);
    cairo_set_source_rgba(cr,.06,.035,.075,.36); cairo_fill(cr);
    PangoLayout *layout = gtk_widget_create_pango_layout(e->canvas, NULL);
    PangoFontDescription *font = pango_font_description_from_string("Sans Bold 10");
    pango_layout_set_font_description(layout, font);
    pango_font_description_free(font);
    for (guint i=0; i<e->markers->len; i++) {
        Marker *m = g_ptr_array_index(e->markers,i);
        gboolean focus = !g_strcmp0(e->selected,m->path);
        const char *text = !g_strcmp0(e->capture,m->path) ? "…" : m->label;
        pango_layout_set_text(layout,text,-1);
        int tw, th; pango_layout_get_pixel_size(layout,&tw,&th);
        int mw = MAX(30,tw+20), mh = 28;
        double x = left+m->u*r.size.width, y = top+m->v*r.size.height;
        GdkRectangle base = {(int)CLAMP(x-mw/2., left, MAX(left,right-mw)),
            (int)CLAMP(y-mh/2.,top,MAX(top,bottom-mh)), mw,mh};
        m->box=base;
        // Two actions may share one native sprite. Keep both labels reachable
        // and connect the displaced label to the measured touch location.
        for (guint step=0; step<e->markers->len*2; step++) {
            gboolean overlaps=FALSE;
            for (guint j=0;j<i && !overlaps;j++) {
                Marker *prior=g_ptr_array_index(e->markers,j);
                overlaps=gdk_rectangle_intersect(&prior->box,&m->box,NULL);
            }
            if (!overlaps) break;
            int shift=(int)(step/2+1)*(mh+3)*(step%2 ? 1 : -1);
            m->box.y=(int)CLAMP(base.y+shift,top,MAX(top,bottom-mh));
        }
        if (m->box.y != base.y) {
            cairo_set_source_rgba(cr,1,.75,.84,.9); cairo_set_line_width(cr,1.5);
            cairo_move_to(cr,x,y); cairo_line_to(cr,m->box.x+mw/2.,m->box.y+mh/2.);
            cairo_stroke(cr);
        }
        pill(cr,m->box);
        if (focus) cairo_set_source_rgba(cr,.43,.09,.24,.98);
        else cairo_set_source_rgba(cr,.085,.055,.10,.95);
        cairo_fill_preserve(cr);
        gboolean custom = g_str_has_prefix(m->slot,"point:");
        if (m->conflict) cairo_set_source_rgb(cr,1,.48,.18);
        else if (focus || custom) cairo_set_source_rgb(cr,1,.36,.58);
        else cairo_set_source_rgba(cr,1,.84,.91,.9);
        cairo_set_line_width(cr,focus?2.5:1.55); cairo_stroke(cr);
        cairo_set_source_rgb(cr,1,.98,.99); cairo_move_to(cr,m->box.x+(mw-tw)/2.,m->box.y+(mh-th)/2.);
        pango_cairo_show_layout(cr,layout);
    }
    g_object_unref(layout);
}

static Marker *hit(ClbbEditor *e, double x,double y) {
    for (guint i=e->markers->len;i>0;i--) {
        Marker *m=g_ptr_array_index(e->markers,i-1); GdkRectangle r=m->box;
        if (x>=r.x && x<r.x+r.width && y>=r.y && y<r.y+r.height) return m;
    }
    return NULL;
}

static void capture(ClbbEditor *e, Marker *m) {
    g_free(e->capture); e->capture=g_strdup(m->path);
    hint(e, "Нажмите клавишу или кнопку мыши    ·    Esc — отмена    ·    Backspace — очистить");
    gtk_widget_grab_focus(e->canvas);
    update_selection(e);
}

static void add_point(GtkButton *button, gpointer data) {
    (void)button;
    ClbbEditor *e=data;
    gtk_popover_popdown(GTK_POPOVER(e->popup));
    g_autofree char *ids=g_strdup(e->cb.read(e->cb.user,"points.list"));
    g_auto(GStrv) rows=g_strsplit(ids?ids:"","\n",-1);
    gboolean used[257]={0};
    for (guint i=0; rows[i]; i++) { int id=atoi(rows[i]); if(id>0&&id<=256)used[id]=TRUE; }
    guint id=1; while(id<=256&&used[id]) id++;
    g_autofree char *command=g_strdup_printf("points.%u.add",id);
    g_autofree char *coords=g_strdup_printf("%u,%u",(guint)(e->add_u*1e6+.5),(guint)(e->add_v*1e6+.5));
    if(id>256 || !e->cb.write(e->cb.user,command,coords)) { hint(e,"Не удалось добавить точку"); return; }
    g_free(e->selected); e->selected=g_strdup_printf("points.%u.key",id);
    refresh(e);
    Marker *m=selected(e); if(m)capture(e,m);
}

static void click(GtkGestureClick *gesture,int count,double x,double y,gpointer data) {
    ClbbEditor *e=data;
    Marker *m=hit(e,x,y);
    guint button=gtk_gesture_single_get_current_button(GTK_GESTURE_SINGLE(gesture));
    if(button==3 && !m) {
        graphene_rect_t r;
        if(!bounds(e,&r)||!graphene_rect_contains_point(&r,&GRAPHENE_POINT_INIT(x,y)))return;
        e->add_u=(x-r.origin.x)/r.size.width; e->add_v=(y-r.origin.y)/r.size.height;
        GdkRectangle at={(int)x,(int)y,1,1};
        gtk_popover_set_pointing_to(GTK_POPOVER(e->popup),&at);
        gtk_popover_popup(GTK_POPOVER(e->popup));
        return;
    }
    if(button!=1 && button!=3)return;
    g_clear_pointer(&e->capture,g_free);
    g_free(e->selected);e->selected=m?g_strdup(m->path):NULL;
    hint(e, m && m->conflict ? "Конфликт клавиш    ·    Дважды ЛКМ — переназначить" : help);
    update_selection(e);
    if(m && button==1 && count==2) capture(e,m);
    if(m && button==3) e->details(e->owner,m->slot);
}

static gboolean tooltip(GtkWidget *widget,int x,int y,gboolean keyboard,GtkTooltip *tooltip,gpointer data) {
    (void)widget; if(keyboard)return FALSE;
    Marker *m=hit(data,x,y); if(!m)return FALSE;
    gtk_tooltip_set_text(tooltip,m->title);
    return TRUE;
}

gboolean clbb_editor_input(ClbbEditor *e,int code,gboolean down,gboolean repeat) {
    if(!e->capture || !code) return FALSE;
    if(!down || repeat)return TRUE;
    if(code==0x01000000) {
        g_clear_pointer(&e->capture,g_free);hint(e,help);
    } else {
        if(code==0x01000003)code=0;
        g_autofree char *value=g_strdup_printf("%d",code);
        if(!e->cb.write(e->cb.user,e->capture,value)) {
            hint(e,"Эту клавишу назначить нельзя. Выберите другую или нажмите Esc.");
            return TRUE;
        }
        g_clear_pointer(&e->capture,g_free);
        refresh(e);
        Marker *m=selected(e);
        hint(e,m && m->conflict ? "Конфликт клавиш    ·    Назначение отключено" : "Назначение сохранено");
    }
    update_selection(e);
    return TRUE;
}

static void gear_clicked(GtkButton *button,gpointer data) {
    (void)button; ClbbEditor *e=data; Marker *m=selected(e);
    if(m)e->details(e->owner,m->slot);
}
static void done_clicked(GtkButton *button,gpointer data) {
    (void)button; ClbbEditor *e=data; clbb_settings_hide(e->owner);
}

ClbbEditor *clbb_editor_new(GtkOverlay *overlay,const ClbbSettingsCallbacks *callbacks,
                            ClbbSettings *owner,ClbbEditorDetails details) {
    ClbbEditor *e=g_new0(ClbbEditor,1);
    e->owner=owner;e->cb=*callbacks;e->details=details;
    e->markers=g_ptr_array_new_with_free_func(free_marker);
    e->viewport=gtk_overlay_get_child(overlay);
    e->layer=gtk_overlay_new(); gtk_widget_set_visible(e->layer,FALSE);
    gtk_overlay_add_overlay(overlay,e->layer);
    gtk_overlay_set_clip_overlay(overlay,e->layer,TRUE);
    e->canvas=gtk_drawing_area_new();gtk_widget_set_focusable(e->canvas,TRUE);
    gtk_widget_set_focus_on_click(e->canvas,TRUE);
    gtk_overlay_set_child(GTK_OVERLAY(e->layer),e->canvas);
    gtk_drawing_area_set_draw_func(GTK_DRAWING_AREA(e->canvas),paint,e,NULL);
    gtk_widget_set_has_tooltip(e->canvas,TRUE);
    g_signal_connect(e->canvas,"query-tooltip",G_CALLBACK(tooltip),e);
    GtkGesture *gesture=gtk_gesture_click_new();
    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture),0);
    g_signal_connect(gesture,"pressed",G_CALLBACK(click),e);
    gtk_widget_add_controller(e->canvas,GTK_EVENT_CONTROLLER(gesture));

    e->popup=gtk_popover_new();
    gtk_widget_set_parent(e->popup,e->canvas);
    GtkWidget *add=gtk_button_new_with_label("＋  Одиночное нажатие");
    gtk_widget_add_css_class(add,"clbb-map-button");
    gtk_popover_set_child(GTK_POPOVER(e->popup),add);
    g_signal_connect(add,"clicked",G_CALLBACK(add_point),e);

    GtkWidget *bar=gtk_box_new(GTK_ORIENTATION_HORIZONTAL,8);
    gtk_widget_set_halign(bar,GTK_ALIGN_END);gtk_widget_set_valign(bar,GTK_ALIGN_START);
    gtk_widget_set_margin_top(bar,10);gtk_widget_set_margin_end(bar,10);
    gtk_widget_add_css_class(bar,"clbb-map-bar");
    e->heading=gtk_label_new("Карта кнопок");gtk_box_append(GTK_BOX(bar),e->heading);
    e->gear=gtk_button_new_from_icon_name("preferences-system-symbolic");
    gtk_widget_set_tooltip_text(e->gear,"Настройки выбранной кнопки");
    gtk_widget_add_css_class(e->gear,"clbb-map-button");
    g_signal_connect(e->gear,"clicked",G_CALLBACK(gear_clicked),e);
    gtk_box_append(GTK_BOX(bar),e->gear);
    GtkWidget *done=gtk_button_new_with_label("Готово");
    gtk_widget_add_css_class(done,"clbb-map-done");
    g_signal_connect(done,"clicked",G_CALLBACK(done_clicked),e);
    gtk_box_append(GTK_BOX(bar),done);gtk_overlay_add_overlay(GTK_OVERLAY(e->layer),bar);
    e->hint=gtk_label_new(help);
    gtk_widget_set_halign(e->hint,GTK_ALIGN_CENTER);gtk_widget_set_valign(e->hint,GTK_ALIGN_END);
    gtk_widget_set_can_target(e->hint,FALSE);gtk_widget_set_margin_bottom(e->hint,9);
    gtk_widget_add_css_class(e->hint,"clbb-map-hint");
    gtk_overlay_add_overlay(GTK_OVERLAY(e->layer),e->hint);
    return e;
}

void clbb_editor_show(ClbbEditor *e) {
    if(clbb_editor_visible(e))return;
    gtk_widget_set_visible(e->layer,TRUE);
    refresh(e);update_selection(e);hint(e,help);
    gtk_widget_grab_focus(e->canvas);
    e->timer=g_timeout_add(110,refresh,e);
}
void clbb_editor_hide(ClbbEditor *e) {
    if(!clbb_editor_visible(e))return;
    g_clear_handle_id(&e->timer,g_source_remove);
    g_clear_pointer(&e->capture,g_free);
    gtk_popover_popdown(GTK_POPOVER(e->popup));
    gtk_widget_set_visible(e->layer,FALSE);
}
gboolean clbb_editor_visible(ClbbEditor *e) {return e && gtk_widget_get_visible(e->layer);}
void clbb_editor_free(ClbbEditor *e) {
    if(!e)return;
    clbb_editor_hide(e);gtk_widget_unparent(e->popup);
    gtk_overlay_remove_overlay(GTK_OVERLAY(gtk_widget_get_parent(e->layer)),e->layer);
    g_free(e->snapshot);g_free(e->selected);g_free(e->capture);
    g_ptr_array_unref(e->markers);g_free(e);
}
