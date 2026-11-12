import { useCallback, useEffect, useState } from "react";

// Тема оформления (раздел 2 плана): тёмная по умолчанию, светлая переключается
// в настройках. Выбор запоминается в localStorage, других настроек у приложения нет.

export type ThemeMode = "dark" | "light";

const STORAGE_KEY = "code-cards.theme";

function readStoredTheme(): ThemeMode {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored === "light" || stored === "dark") {
      return stored;
    }
  } catch {
    // localStorage может быть недоступен: тогда остаётся тёмная тема.
  }
  return "dark";
}

/** Текущая тема и переключатель с сохранением в localStorage. */
export function useThemeMode(): {
  mode: ThemeMode;
  setMode: (mode: ThemeMode) => void;
} {
  const [mode, setModeState] = useState<ThemeMode>(readStoredTheme);

  useEffect(() => {
    // Отмечаем тему в корне, чтобы CSS знал о светлом варианте.
    document.documentElement.dataset.theme = mode;
    document.documentElement.style.colorScheme = mode;
    try {
      window.localStorage.setItem(STORAGE_KEY, mode);
    } catch {
      // Запоминать не обязательно: тема работает и без сохранения.
    }
  }, [mode]);

  const setMode = useCallback((next: ThemeMode) => setModeState(next), []);

  return { mode, setMode };
}
