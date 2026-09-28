// Shared helpers for the overlay and the HUD.

/** Kelvin → RGB (Tanner Helland's approximation), normalised to full brightness. */
export function kelvinToRgb(kelvin) {
  const t = kelvin / 100;
  let r, g, b;
  if (t <= 66) {
    r = 255;
    g = 99.4708025861 * Math.log(t) - 161.1195681661;
    b = t <= 19 ? 0 : 138.5177312231 * Math.log(t - 10) - 305.0447927307;
  } else {
    r = 329.698727446 * Math.pow(t - 60, -0.1332047592);
    g = 288.1221695283 * Math.pow(t - 60, -0.0755148492);
    b = 255;
  }
  const c = (v) => Math.max(0, Math.min(255, v));
  [r, g, b] = [c(r), c(g), c(b)];
  const k = 255 / Math.max(r, g, b);
  return [Math.round(r * k), Math.round(g * k), Math.round(b * k)];
}

export const rgb = ([r, g, b], a = 1) => `rgba(${r}, ${g}, ${b}, ${a})`;

export const tauri = window.__TAURI__;
