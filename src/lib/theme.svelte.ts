export type Theme = "light" | "dark" | "system";

const KEY = "nju-theme";

class ThemeStore {
  mode = $state<Theme>("system");
  /** Thème effectif (mode explicite ou préférence système), utilisé par les composants. */
  isDark = $state(false);

  init() {
    try {
      const saved = localStorage.getItem(KEY);
      if (saved === "light" || saved === "dark" || saved === "system") this.mode = saved;
    } catch {}
    this.apply();
    matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => this.apply());
  }

  set(mode: Theme) {
    this.mode = mode;
    try {
      localStorage.setItem(KEY, mode);
    } catch {}
    this.apply();
  }

  private apply() {
    const dark =
      this.mode === "dark" ||
      (this.mode === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
    this.isDark = dark;
    document.documentElement.classList.toggle("dark", dark);
  }
}

export const theme = new ThemeStore();