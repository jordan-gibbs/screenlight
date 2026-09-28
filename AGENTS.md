# Installing Screenlight (instructions for coding agents)

Screenlight is a Tauri 2 app (Rust + static HTML in `ui/`, no JS bundler).
You must build it on the OS it will run on; cross-compiling isn't supported.

## 1. Prerequisites

All platforms: **Rust** (stable, via rustup) and **Node.js 18+**.

| OS | Extra |
|---|---|
| Windows 10/11 | MSVC Build Tools ("Desktop development with C++"). WebView2 already ships with Windows 11. |
| macOS 11+ | `xcode-select --install` |
| Debian/Ubuntu | `sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev` |
| Fedora | `sudo dnf install webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel librsvg2-devel` + `sudo dnf group install c-development` |
| Arch | `sudo pacman -S webkit2gtk-4.1 base-devel openssl libappindicator-gtk3 librsvg` |

## 2. Build

```sh
npm install
npx tauri build --no-bundle
```

The binary lands in `src-tauri/target/release/` as `screenlight` (`screenlight.exe` on Windows).
(`npm run build` also makes an installer: NSIS on Windows. Add `"app"`/`"dmg"` or `"deb"`/`"appimage"` to `bundle.targets` in `src-tauri/tauri.conf.json` for macOS/Linux installers.)

## 3. Install for the user

- **Windows:** copy `screenlight.exe` to `%LOCALAPPDATA%\Programs\Screenlight\`. For launch at login, add a shortcut to it in `shell:startup`.
- **macOS:** `npx tauri build --bundles app`, then copy `src-tauri/target/release/bundle/macos/Screenlight.app` to `/Applications`. For launch at login, add it under System Settings → General → Login Items.
- **Linux:** copy the binary to `~/.local/bin/`. For launch at login, add a `~/.config/autostart/screenlight.desktop` whose `Exec=` points at it.

Then run it. It lives in the tray/menu bar and has no main window.

## Platform notes

- **Linux + Wayland:** Wayland doesn't let apps position always-on-top overlays. The light works best on X11, or run it with `GDK_BACKEND=x11`. On X11 you need a compositor for transparency (every major desktop has one).
- **Linux:** "Hide from screen capture" does nothing. The camera detection only sees processes owned by the current user (`/proc/*/fd` → `/dev/video*`).
- **macOS:** transparency uses `macOSPrivateApi` (fine for self-built apps, but not allowed in the Mac App Store).
