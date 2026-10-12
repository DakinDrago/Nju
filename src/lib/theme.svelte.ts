export type Theme = "light" | "dark" | "system";

const KEY = "nju-theme";

class ThemeStore {
  mode = $state<Theme>("system");

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
    document.documentElement.classList.toggle("dark", dark);
  }
}

export const theme = new ThemeStore();