/** The window's theme: dark, light, or as the system is; kept in this window's storage. */
export type Theme = "system" | "dark" | "light";
export const THEMES: readonly Theme[] = ["system", "dark", "light"];

const KEY = "dub-studio:theme";

export function savedTheme(): Theme {
  try {
    const t = localStorage.getItem(KEY);
    return t === "dark" || t === "light" ? t : "system";
  } catch {
    return "system";
  }
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}

export function setTheme(theme: Theme): void {
  try { localStorage.setItem(KEY, theme); } catch { /* the theme is only remembered where storage works */ }
  applyTheme(theme);
}
