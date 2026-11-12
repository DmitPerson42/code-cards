import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ConfigProvider, App as AntApp } from "antd";
import ruRU from "antd/locale/ru_RU";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { darkTheme, lightTheme } from "./theme/antdTheme";
import { useThemeMode } from "./hooks/useTheme";
import "./styles.css";

// Один клиент запросов на всё приложение. Данные локальные, поэтому
// повторные запросы при фокусе окна не нужны.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      refetchOnWindowFocus: false,
      staleTime: 5_000,
    },
  },
});

/** Корень приложения: тема берётся из настроек и попадает в ConfigProvider. */
function Root() {
  const theme = useThemeMode();
  return (
    <QueryClientProvider client={queryClient}>
      <ConfigProvider
        theme={theme.mode === "dark" ? darkTheme : lightTheme}
        locale={ruRU}
      >
        <AntApp>
          <App themeMode={theme.mode} onThemeModeChange={theme.setMode} />
        </AntApp>
      </ConfigProvider>
    </QueryClientProvider>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
