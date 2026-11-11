// SPDX-License-Identifier: GPL-3.0-or-later
//! Standalone update window; Android is already stopped. GTK is only the shell.
use clbb_mapper::updater::Updates;
use std::{
    ffi::{CString, c_char, c_void},
    path::PathBuf,
    process::Command,
};
type P = *mut c_void;
#[link(name = "gtk-4")]
unsafe extern "C" {
    fn gtk_init();
    fn gtk_window_new() -> P;
    fn gtk_window_set_title(w: P, t: *const c_char);
    fn gtk_window_set_default_size(w: P, x: i32, y: i32);
    fn gtk_window_set_child(w: P, c: P);
    fn gtk_window_present(w: P);
    fn gtk_window_destroy(w: P);
    fn gtk_box_new(orientation: i32, spacing: i32) -> P;
    fn gtk_box_append(b: P, c: P);
    fn gtk_label_new(t: *const c_char) -> P;
    fn gtk_label_set_text(w: P, t: *const c_char);
    fn gtk_label_set_wrap(w: P, v: i32);
    fn gtk_label_set_xalign(w: P, v: f32);
    fn gtk_button_new_with_label(t: *const c_char) -> P;
    fn gtk_button_set_label(w: P, t: *const c_char);
    fn gtk_progress_bar_new() -> P;
    fn gtk_progress_bar_set_fraction(w: P, v: f64);
    fn gtk_progress_bar_pulse(w: P);
    fn gtk_progress_bar_set_show_text(w: P, v: i32);
    fn gtk_progress_bar_set_text(w: P, t: *const c_char);
    fn gtk_widget_set_sensitive(w: P, v: i32);
    fn gtk_widget_add_css_class(w: P, t: *const c_char);
    fn gtk_widget_set_margin_top(w: P, v: i32);
    fn gtk_widget_set_margin_bottom(w: P, v: i32);
    fn gtk_widget_set_margin_start(w: P, v: i32);
    fn gtk_widget_set_margin_end(w: P, v: i32);
    fn gtk_css_provider_new() -> P;
    fn gtk_css_provider_load_from_data(p: P, data: *const c_char, len: isize);
    fn gtk_style_context_add_provider_for_display(d: P, p: P, priority: u32);
    fn gdk_display_get_default() -> P;
}
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_main_loop_new(c: P, r: i32) -> P;
    fn g_main_loop_run(l: P);
    fn g_main_loop_quit(l: P);
    fn g_main_loop_unref(l: P);
    fn g_timeout_add(ms: u32, f: Option<unsafe extern "C" fn(P) -> i32>, data: P) -> u32;
    fn g_source_remove(id: u32) -> i32;
}
#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_signal_connect_data(
        o: P,
        s: *const c_char,
        f: Option<unsafe extern "C" fn()>,
        data: P,
        destroy: P,
        flags: u32,
    ) -> u64;
}
fn c(s: &str) -> CString {
    CString::new(s.replace('\0', " ")).unwrap()
}
struct Ui {
    loop_: P,
    status: P,
    version: P,
    bar: P,
    action: P,
    cancel: P,
    updates: Updates,
    phase: u8,
    self_test: bool,
    expected_error: bool,
    progress_seen: bool,
    root: PathBuf,
}
unsafe fn text(w: P, s: &str) {
    unsafe {
        gtk_label_set_text(w, c(s).as_ptr());
    }
}
unsafe extern "C" fn close(_: P, data: P) -> i32 {
    unsafe {
        let u = &mut *(data as *mut Ui);
        if u.phase == 1 {
            return 1;
        }
        g_main_loop_quit(u.loop_);
        1
    }
}
unsafe extern "C" fn cancel(_: P, data: P) {
    unsafe {
        close(std::ptr::null_mut(), data);
    }
}
unsafe extern "C" fn action(_: P, data: P) {
    unsafe {
        let u = &mut *(data as *mut Ui);
        if u.phase == 2 {
            let root = if std::env::var("CLBB_MANAGED_INSTALL").as_deref() == Ok("1") {
                let install = std::env::var_os("CLBB_INSTALL_ROOT")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        std::env::var_os("XDG_DATA_HOME")
                            .map(PathBuf::from)
                            .unwrap_or_else(|| {
                                PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                                    .join(".local/share")
                            })
                            .join("clbb")
                    });
                match install.join("current").canonicalize() {
                    Ok(root) => root,
                    Err(error) => {
                        text(
                            u.status,
                            &format!("Не удалось найти установленную версию: {error}"),
                        );
                        return;
                    }
                }
            } else {
                u.root.clone()
            };
            match Command::new(root.join("bin/clbb-launcher"))
                .env("CLBB_ROOT", &root)
                .env("CLBB_ATL_ROOT", &root)
                .env_remove("CLBB_NATIVE_LOCK_HELD")
                .spawn()
            {
                Ok(_) => g_main_loop_quit(u.loop_),
                Err(e) => text(u.status, &format!("Не удалось запустить игру: {e}")),
            }
        } else if u.phase == 0 && u.updates.request("updates.install") {
            u.phase = 1;
            gtk_widget_set_sensitive(u.action, 0);
            gtk_widget_set_sensitive(u.cancel, 0);
            text(u.status, "Подготовка обновления…");
        } else if u.phase == 0 {
            text(u.status, &u.updates.read("updates.status"));
        }
    }
}
unsafe extern "C" fn poll(data: P) -> i32 {
    unsafe {
        let u = &mut *(data as *mut Ui);
        if u.phase != 1 {
            return 1;
        }
        text(u.status, &u.updates.read("updates.status"));
        let progress = u
            .updates
            .read("updates.progress")
            .parse::<f64>()
            .unwrap_or(-1.);
        if progress >= 0. {
            u.progress_seen = true;
            gtk_progress_bar_set_fraction(u.bar, progress / 100.);
            gtk_progress_bar_set_text(u.bar, c(&format!("{progress:.1}%")).as_ptr());
        } else {
            gtk_progress_bar_pulse(u.bar);
            gtk_progress_bar_set_text(u.bar, c("Выполняется…").as_ptr());
        }
        if u.updates.read("updates.busy") == "false" {
            gtk_widget_set_sensitive(u.cancel, 1);
            gtk_widget_set_sensitive(u.action, 1);
            match u.updates.read("updates.outcome").as_str() {
                "installed" | "current" => {
                    u.phase = 2;
                    gtk_progress_bar_set_fraction(u.bar, 1.);
                    gtk_progress_bar_set_text(u.bar, c("100%").as_ptr());
                    let installed = u.updates.read("updates.installed_version");
                    text(u.version, &format!("Версия: {installed}"));
                    text(
                        u.status,
                        if u.updates.read("updates.outcome") == "current" {
                            "Установлена актуальная версия. Запустить игру?"
                        } else {
                            "Обновление завершено. Запустить игру?"
                        },
                    );
                    gtk_button_set_label(u.action, c("Запустить CLBB").as_ptr());
                    gtk_button_set_label(u.cancel, c("Закрыть").as_ptr());
                }
                _ => {
                    u.phase = 0;
                    gtk_button_set_label(u.action, c("Повторить").as_ptr());
                }
            }
            if u.self_test {
                assert_eq!(u.phase, if u.expected_error { 0 } else { 2 });
                assert!(u.progress_seen, "fixture progress must reach the window");
                println!(
                    "CLBB_UPDATE_WINDOW_OK progress,status,outcome={}",
                    u.updates.read("updates.outcome")
                );
                g_main_loop_quit(u.loop_);
            }
        }
        1
    }
}
fn main() {
    unsafe {
        let test = std::env::args().any(|a| a == "--self-test");
        // Self-test uses the caller's explicitly supplied local fake manager. Never
        // choose a network/install command by itself.
        let cancel_test = std::env::args().any(|a| a == "--self-test-cancel");
        let error_test = std::env::args().any(|a| a == "--self-test-error");
        let test = test || error_test;
        if test || cancel_test {
            assert!(
                std::env::var("CLBB_UPDATE_MANAGER")
                    .unwrap_or_default()
                    .starts_with("/tmp/clbb-update-ui-test-")
            );
        }
        gtk_init();
        let window = gtk_window_new();
        gtk_window_set_title(window, c("CLBB — Обновление").as_ptr());
        gtk_window_set_default_size(window, 620, 300);
        let provider = gtk_css_provider_new();
        let css = c(
            "headerbar{background:#1c1b22;color:#f4f3f6;border-bottom:1px solid #35313e;} label.clbb-updater-title{font-size:22px;font-weight:600;} window{background:#141419;color:#f4f3f6;} label{color:#f4f3f6;} button{background:#292831;color:#f4f3f6;border:1px solid #44414d;padding:10px 22px;} button.suggested-action{background:#a72c46;border-color:#c64960;} progressbar text{color:#d8d4de;} progressbar trough{border:1px solid #48414f;border-radius:4px;background:#302d36;min-height:10px;} progressbar progress{border-radius:4px;background:#c74760;min-height:10px;}",
        );
        gtk_css_provider_load_from_data(provider, css.as_ptr(), -1);
        gtk_style_context_add_provider_for_display(gdk_display_get_default(), provider, 600);
        let body = gtk_box_new(1, 18);
        gtk_widget_set_margin_top(body, 24);
        gtk_widget_set_margin_bottom(body, 24);
        gtk_widget_set_margin_start(body, 24);
        gtk_widget_set_margin_end(body, 24);
        gtk_window_set_child(window, body);
        let title = gtk_label_new(c("Обновление CLBB").as_ptr());
        gtk_label_set_xalign(title, 0.);
        gtk_widget_add_css_class(title, c("clbb-updater-title").as_ptr());
        gtk_box_append(body, title);
        let bar = gtk_progress_bar_new();
        gtk_progress_bar_set_show_text(bar, 1);
        gtk_progress_bar_set_text(bar, c("Готово к обновлению").as_ptr());
        gtk_box_append(body, bar);
        let updates = Updates::new();
        let version = gtk_label_new(
            c(&format!(
                "Текущая версия: {}",
                updates.read("updates.version")
            ))
            .as_ptr(),
        );
        gtk_label_set_xalign(version, 0.);
        gtk_box_append(body, version);
        let status = gtk_label_new(
            c("Обновить CLBB? После завершения можно будет снова запустить игру.").as_ptr(),
        );
        gtk_label_set_wrap(status, 1);
        gtk_label_set_xalign(status, 0.);
        gtk_box_append(body, status);
        let row = gtk_box_new(0, 12);
        gtk_box_append(body, row);
        let cancel_b = gtk_button_new_with_label(c("Отмена").as_ptr());
        let action_b = gtk_button_new_with_label(c("Обновиться").as_ptr());
        gtk_widget_add_css_class(action_b, c("suggested-action").as_ptr());
        gtk_box_append(row, cancel_b);
        gtk_box_append(row, action_b);
        let managed = std::env::var("CLBB_MANAGED_INSTALL").as_deref() == Ok("1");
        if !managed {
            text(
                status,
                "Эта локальная сборка обновляется разработчиком. Автообновление доступно в версии, установленной установщиком.",
            );
            gtk_widget_set_sensitive(action_b, 0);
        }
        let loop_ = g_main_loop_new(std::ptr::null_mut(), 0);
        let root = std::env::var_os("CLBB_ROOT")
            .map(PathBuf::from)
            .unwrap_or_default();
        let data = Box::into_raw(Box::new(Ui {
            loop_,
            status,
            version,
            bar,
            action: action_b,
            cancel: cancel_b,
            updates,
            phase: 0,
            self_test: test,
            expected_error: error_test,
            progress_seen: false,
            root,
        })) as P;
        let clicked = |widget: P, callback: unsafe extern "C" fn(P, P)| {
            g_signal_connect_data(
                widget,
                c("clicked").as_ptr(),
                Some(std::mem::transmute::<
                    unsafe extern "C" fn(P, P),
                    unsafe extern "C" fn(),
                >(callback)),
                data,
                std::ptr::null_mut(),
                0,
            )
        };
        clicked(cancel_b, cancel);
        clicked(action_b, action);
        g_signal_connect_data(
            window,
            c("close-request").as_ptr(),
            Some(std::mem::transmute::<
                unsafe extern "C" fn(P, P) -> i32,
                unsafe extern "C" fn(),
            >(close)),
            data,
            std::ptr::null_mut(),
            0,
        );
        let timer = g_timeout_add(80, Some(poll), data);
        gtk_window_present(window);
        if test {
            action(std::ptr::null_mut(), data);
            assert_eq!((*(data as *mut Ui)).phase, 1);
        }
        if cancel_test {
            cancel(std::ptr::null_mut(), data);
            println!("CLBB_UPDATE_WINDOW_CANCEL_OK no-install-started");
        } else {
            g_main_loop_run(loop_);
        }
        if test && !error_test && std::env::var("CLBB_UPDATE_UI_TEST_RESTART").as_deref() == Ok("1")
        {
            action(std::ptr::null_mut(), data);
        }
        g_source_remove(timer);
        gtk_window_destroy(window);
        g_main_loop_unref(loop_);
        drop(Box::from_raw(data as *mut Ui));
    }
}
