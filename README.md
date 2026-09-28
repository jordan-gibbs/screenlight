# Screenlight

A soft ring light around the edges of your screen, like a phone's front-camera flash, for your desktop. It's a single ~3.5 MB native binary (Tauri 2: Rust + a web UI).

- **Ctrl + Alt + L** toggles the light and briefly shows the HUD.
- **Tray icon**: left-click toggles the light. Right-click opens the menu (Light / Adjust… / Quit).
- **HUD**: Intensity (0–100% opacity), Warmth (2000–9000 K), Width, Softness. It fades out on its own when you're not hovering it.
- Covers every display. Sizes scale with each display, so it looks the same on a laptop and on an ultrawide.
- **Auto-on with camera**: lights up when any app starts using a camera, and turns back off when the camera stops. The hotkey turns it off instantly, and it stays off until the next time a camera starts.
- Click-through, never takes focus, and is hidden from screen shares and recordings by default.

## Settings

`%APPDATA%\com.jordangibbs.screenlight\settings.json` (macOS/Linux use the platform config dir):

```json
{ "intensity": 0.9, "kelvin": 5200, "width": 7, "softness": 0.65,
  "hotkey": "Ctrl+Alt+L", "hide_from_capture": true, "auto_camera": true }
```

## Install

Windows, macOS and Linux are supported. Build on the machine you'll run it on; see [AGENTS.md](AGENTS.md) for per-OS prerequisites and install steps. Or just tell your coding agent: *"install this."*

```sh
npm install
npx tauri build --no-bundle   # → src-tauri/target/release/screenlight(.exe)
npx tauri dev                 # run from source
```

## Layout

- `src-tauri/src/main.rs`: tray, hotkey, per-monitor overlay windows, settings
- `ui/overlay.html`: the light itself (layered inset box-shadows)
- `ui/hud.html`: the heads-up control panel
- `src-tauri/src/camera.rs`: camera-in-use detection per OS (registry / CoreMediaIO / `/proc`)
- `ui/light.js`: Kelvin → RGB
