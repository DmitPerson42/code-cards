import type { ThemeConfig } from "antd";
import { theme } from "antd";

// Тёмная тема по умолчанию (раздел 2 плана), светлая переключается в настройках.
export const darkTheme: ThemeConfig = {
  algorithm: theme.darkAlgorithm,
  token: {
    colorPrimary: "#5aaaff",
    borderRadius: 6,
    fontFamily:
      "'Segoe UI', 'Inter', system-ui, -apple-system, 'Roboto', sans-serif",
    fontSize: 14,
  },
  components: {
    Layout: {
      headerBg: "#161a24",
      siderBg: "#161a24",
      bodyBg: "#10131a",
    },
    Card: {
      colorBgContainer: "#1e2330",
    },
  },
};

export const lightTheme: ThemeConfig = {
  algorithm: theme.defaultAlgorithm,
  token: {
    colorPrimary: "#1f6fd0",
    borderRadius: 6,
    fontFamily:
      "'Segoe UI', 'Inter', system-ui, -apple-system, 'Roboto', sans-serif",
    fontSize: 14,
  },
  components: {
    Layout: {
      headerBg: "#ffffff",
      siderBg: "#f5f6f8",
    },
  },
};

/** Моноширинный шрифт для кода и разбора. */
export const MONO_FONT =
  "'Cascadia Mono', 'Consolas', 'JetBrains Mono', 'Courier New', monospace";
