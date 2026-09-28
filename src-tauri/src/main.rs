#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};

mod camera;

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, Wry,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const DEFAULT_HOTKEY: &str = "Ctrl+Alt+L";
const HUD_W: f64 = 380.0;
const HUD_H: f64 = 430.0;
const FADE_MS: u64 = 420;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    /// 0.0 – 1.0
    intensity: f64,
    /// Kelvin, 2000 – 9000
    kelvin: f64,
    /// Band width as % of the shorter screen side
    width: f64,
    /// 0.0 (hard edge) – 1.0 (long falloff into the center)
    softness: f64,
    hotkey: String,
    /// Exclude the light from screen shares & recordings.
    hide_from_capture: bool,
    /// Turn on automatically while any camera is in use.
    auto_camera: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            intensity: 0.9,
            kelvin: 5200.0,
            width: 7.0,
            softness: 0.65,
            hotkey: DEFAULT_HOTKEY.into(),
            hide_from_capture: true,
            auto_camera: true,
        }
    }
}

#[derive(Clone, Serialize)]
struct Snapshot {
    on: bool,
    #[serde(flatten)]
    settings: Settings,
}

struct Shared {
    on: Mutex<bool>,
    settings: Mutex<Settings>,
    hide_gen: AtomicU64,
    save_gen: AtomicU64,
    /// True when the camera watcher (not the user) turned the light on.
    auto_lit: Mutex<bool>,
    toggle_item: Mutex<Option<CheckMenuItem<Wry>>>,
    auto_item: Mutex<Option<CheckMenuItem<Wry>>>,
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Debounced write so dragging a slider doesn't hammer the disk.
fn save_settings_soon(app: &AppHandle) {
    let shared = app.state::<Shared>();
    let gen = shared.save_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(400));
        let shared = app.state::<Shared>();
        if shared.save_gen.load(Ordering::SeqCst) != gen {
            return;
        }
        let Some(path) = settings_path(&app) else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let json = serde_json::to_string_pretty(&*shared.settings.lock().unwrap()).unwrap();
        let _ = fs::write(path, json);
    });
}

fn snapshot(app: &AppHandle) -> Snapshot {
    let shared = app.state::<Shared>();
    let on = *shared.on.lock().unwrap();
    let settings = shared.settings.lock().unwrap().clone();
    Snapshot { on, settings }
}

fn broadcast(app: &AppHandle) {
    let _ = app.emit("state", snapshot(app));
}

fn is_overlay(label: &str) -> bool {
    label.starts_with("light-")
}

/// One click-through, transparent, always-on-top window per display,
/// re-fitted every time the light turns on so display changes are picked up.
fn fit_overlays(app: &AppHandle) {
    let hide_from_capture = app.state::<Shared>().settings.lock().unwrap().hide_from_capture;
    let monitors = app.available_monitors().unwrap_or_default();
    for (i, m) in monitors.iter().enumerate() {
        let label = format!("light-{i}");
        let win = match app.get_webview_window(&label) {
            Some(w) => w,
            None => {
                match WebviewWindowBuilder::new(app, &label, WebviewUrl::App("overlay.html".into()))
                    .title("Screenlight")
                    .decorations(false)
                    .transparent(true)
                    .shadow(false)
                    .resizable(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .visible_on_all_workspaces(true)
                    .focusable(false)
                    .focused(false)
                    .visible(false)
                    .build()
                {
                    Ok(w) => w,
                    Err(_) => continue,
                }
            }
        };
        let _ = win.set_content_protected(hide_from_capture);
        let _ = win.set_position(*m.position());
        let _ = win.set_size(*m.size());
    }
    for (label, win) in app.webview_windows() {
        if let Some(idx) = label.strip_prefix("light-").and_then(|n| n.parse::<usize>().ok()) {
            if idx >= monitors.len() {
                let _ = win.destroy();
            }
        }
    }
}

/// Overlays and HUD are both topmost; re-assert so the HUD stays in front.
fn raise_hud(app: &AppHandle) {
    if let Some(hud) = app.get_webview_window("hud") {
        if hud.is_visible().unwrap_or(false) {
            let _ = hud.set_always_on_top(false);
            let _ = hud.set_always_on_top(true);
        }
    }
}

fn set_light(app: &AppHandle, on: bool) {
    apply_light(app, on, false);
}

fn apply_light(app: &AppHandle, on: bool, auto: bool) {
    let shared = app.state::<Shared>();
    *shared.on.lock().unwrap() = on;
    *shared.auto_lit.lock().unwrap() = on && auto;
    let gen = shared.hide_gen.fetch_add(1, Ordering::SeqCst) + 1;

    if on {
        fit_overlays(app);
        for (label, win) in app.webview_windows() {
            if is_overlay(&label) {
                let _ = win.show();
                // Must follow show(): on Linux the native window only exists once shown.
                let _ = win.set_ignore_cursor_events(true);
                let _ = win.set_always_on_top(true);
            }
        }
        raise_hud(app);
    } else {
        // Let the overlay fade out, then actually hide the windows.
        let app = app.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(FADE_MS));
            if app.state::<Shared>().hide_gen.load(Ordering::SeqCst) == gen {
                for (label, win) in app.webview_windows() {
                    if is_overlay(&label) {
                        let _ = win.hide();
                    }
                }
            }
        });
    }

    if let Some(item) = shared.toggle_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(on);
    }
    broadcast(app);
}

fn toggle_light(app: &AppHandle) {
    let on = *app.state::<Shared>().on.lock().unwrap();
    set_light(app, !on);
}

/// Show the HUD at the bottom-center of the display under the cursor.
/// `linger` is how long (ms) it stays before fading if left untouched.
fn show_hud(app: &AppHandle, linger: u32) {
    let Some(hud) = app.get_webview_window("hud") else { return };
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let scale = m.scale_factor();
        let area = m.work_area();
        let w = (HUD_W * scale) as i32;
        let h = (HUD_H * scale) as i32;
        let x = area.position.x + (area.size.width as i32 - w) / 2;
        let y = area.position.y + area.size.height as i32 - h - (2.0 * scale) as i32;
        let _ = hud.set_position(PhysicalPosition::new(x, y));
    }
    let _ = hud.show();
    raise_hud(app);
    let _ = hud.emit("hud-show", linger);
}

#[tauri::command]
fn get_state(app: AppHandle) -> Snapshot {
    snapshot(&app)
}

#[tauri::command]
fn set_on(app: AppHandle, on: bool) {
    set_light(&app, on);
}

#[tauri::command]
fn update_settings(app: AppHandle, intensity: f64, kelvin: f64, width: f64, softness: f64) {
    {
        let shared = app.state::<Shared>();
        let mut s = shared.settings.lock().unwrap();
        s.intensity = intensity.clamp(0.0, 1.0);
        s.kelvin = kelvin.clamp(2000.0, 9000.0);
        s.width = width.clamp(1.0, 30.0);
        s.softness = softness.clamp(0.0, 1.0);
    }
    broadcast(&app);
    save_settings_soon(&app);
}

/// Acts only on camera on/off *edges*, so turning the light off with the
/// hotkey mid-call sticks until the next time a camera starts.
fn watch_camera(app: AppHandle) {
    thread::spawn(move || {
        let mut was_live = false;
        loop {
            let live = camera::in_use();
            if live != was_live {
                was_live = live;
                let shared = app.state::<Shared>();
                let enabled = shared.settings.lock().unwrap().auto_camera;
                let on = *shared.on.lock().unwrap();
                let auto_lit = *shared.auto_lit.lock().unwrap();
                if live && enabled && !on {
                    apply_light(&app, true, true);
                    show_hud(&app, 2200);
                } else if !live && auto_lit {
                    set_light(&app, false);
                }
            }
            thread::sleep(Duration::from_millis(1200));
        }
    });
}

fn set_auto_camera_inner(app: &AppHandle, enabled: bool) {
    let shared = app.state::<Shared>();
    shared.settings.lock().unwrap().auto_camera = enabled;
    if let Some(item) = shared.auto_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(enabled);
    }
    broadcast(app);
    save_settings_soon(app);
}

#[tauri::command]
fn set_auto_camera(app: AppHandle, enabled: bool) {
    set_auto_camera_inner(&app, enabled);
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

fn register_hotkey(app: &AppHandle) {
    let wanted = app.state::<Shared>().settings.lock().unwrap().hotkey.clone();
    let gs = app.global_shortcut();
    if gs.register(wanted.as_str()).is_err() && wanted != DEFAULT_HOTKEY {
        let _ = gs.register(DEFAULT_HOTKEY);
        app.state::<Shared>().settings.lock().unwrap().hotkey = DEFAULT_HOTKEY.into();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle = CheckMenuItem::with_id(app, "toggle", "Light", true, false, None::<&str>)?;
    let auto_on = app.state::<Shared>().settings.lock().unwrap().auto_camera;
    let auto = CheckMenuItem::with_id(app, "auto", "Auto-on with camera", true, auto_on, None::<&str>)?;
    let adjust = MenuItem::with_id(app, "adjust", "Adjust…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Screenlight", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&toggle, &auto, &adjust, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    *app.state::<Shared>().toggle_item.lock().unwrap() = Some(toggle);
    *app.state::<Shared>().auto_item.lock().unwrap() = Some(auto);

    let hotkey = app.state::<Shared>().settings.lock().unwrap().hotkey.clone();
    TrayIconBuilder::with_id("tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip(format!("Screenlight — {hotkey}"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => {
                toggle_light(app);
                show_hud(app, 1800);
            }
            "auto" => {
                let enabled = !app.state::<Shared>().settings.lock().unwrap().auto_camera;
                set_auto_camera_inner(app, enabled);
            }
            "adjust" => show_hud(app, 5000),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                toggle_light(app);
                show_hud(app, 2400);
            }
        })
        .build(app)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_hud(app, 5000)))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_light(app);
                        show_hud(app, 1600);
                    }
                })
                .build(),
        )
        .manage(Shared {
            on: Mutex::new(false),
            settings: Mutex::new(Settings::default()),
            hide_gen: AtomicU64::new(0),
            save_gen: AtomicU64::new(0),
            auto_lit: Mutex::new(false),
            toggle_item: Mutex::new(None),
            auto_item: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![get_state, set_on, update_settings, set_auto_camera, quit])
        .setup(|app| {
            let handle = app.handle().clone();
            *app.state::<Shared>().settings.lock().unwrap() = load_settings(&handle);

            WebviewWindowBuilder::new(app, "hud", WebviewUrl::App("hud.html".into()))
                .title("Screenlight")
                .inner_size(HUD_W, HUD_H)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .resizable(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .focusable(false)
                .focused(false)
                .visible(false)
                .build()?;

            register_hotkey(&handle);
            build_tray(&handle)?;
            // Warm up overlay windows so the first toggle is instant.
            fit_overlays(&handle);
            show_hud(&handle, 2600);
            watch_camera(handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Screenlight");
}
