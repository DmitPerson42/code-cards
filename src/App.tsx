import { useCallback, useEffect, useRef, useState } from "react";
import { Alert, Button, Flex, Layout, Space, Tooltip, Typography } from "antd";
import { SettingOutlined } from "@ant-design/icons";
import { useCard } from "./hooks/useCards";
import type { ThemeMode } from "./hooks/useTheme";
import { errorText, type CardDetail } from "./api/tauri";
import CardList from "./components/CardList";
import CardEditor, { emptyCard } from "./components/CardEditor";
import CardView from "./components/CardView";
import Settings from "./components/Settings";

// Экран приложения: список карточек, просмотр, редактор и настройки.
// Горячие клавиши раздела 3.7 обслуживаются здесь, потому что им нужен
// доступ к текущему экрану: Ctrl+N, Ctrl+F, Ctrl+S, Esc, Ctrl+Shift+C.

type Screen =
  | { name: "list" }
  | { name: "view"; id: number }
  | { name: "edit"; id: number | null };

export interface AppProps {
  themeMode: ThemeMode;
  onThemeModeChange: (mode: ThemeMode) => void;
}

export default function App({ themeMode, onThemeModeChange }: AppProps) {
  const [screen, setScreen] = useState<Screen>({ name: "list" });
  const [settingsOpen, setSettingsOpen] = useState(false);

  // Запрос карточки нужен на двух экранах: просмотр и правка. Раньше id
  // брался только для просмотра, и редактор оставался с card.data = null:
  // вместо формы пользователь видел "Карточка загружается" навсегда.
  const openedId =
    screen.name === "view"
      ? screen.id
      : screen.name === "edit"
        ? screen.id
        : null;
  const card = useCard(openedId);

  // Ссылка на обработчик сохранения активного редактора: кнопка и Ctrl+S
  // должны вызывать одно и то же.
  const saveHandlerRef = useRef<(() => void) | null>(null);
  // Esc закрывает редактор без сохранения: подтверждение показывает сам редактор.
  const cancelHandlerRef = useRef<(() => void) | null>(null);

  const goList = useCallback(() => setScreen({ name: "list" }), []);
  const openCard = useCallback((id: number) => setScreen({ name: "view", id }), []);
  const createCard = useCallback(() => setScreen({ name: "edit", id: null }), []);
  const editCard = useCallback((id: number | null) => setScreen({ name: "edit", id }), []);
  // После сохранения открываем карточку по id, который вернула команда.
  const savedCard = useCallback((id: number) => setScreen({ name: "view", id }), []);

  const editing: CardDetail | null =
    screen.name === "edit"
      ? screen.id === null
        ? emptyCard()
        : card.data ?? null
      : null;

  // Горячие клавиши раздела 3.7 плана.
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      const ctrl = event.ctrlKey || event.metaKey;

      // Ctrl+S работает и внутри кода и полей: сохраняет карточку.
      if (ctrl && !event.shiftKey && event.key.toLowerCase() === "s" && screen.name === "edit") {
        event.preventDefault();
        saveHandlerRef.current?.();
        return;
      }

      // Ctrl+Shift+C копирует код открытой карточки.
      if (ctrl && event.shiftKey && event.key.toLowerCase() === "c" && screen.name === "view") {
        event.preventDefault();
        void copyOpenedCardCode();
        return;
      }

      // Esc закрывает редактор без сохранения. В полях ввода Esc не перехватываем,
      // иначе он ломает закрытие выпадающих списков.
      if (event.key === "Escape" && screen.name === "edit" && !isTextInput(event.target)) {
        event.preventDefault();
        cancelHandlerRef.current?.();
        return;
      }

      // Ctrl+N и Ctrl+F обрабатывает список карточек: им нужен фокус в поиске.
      if (ctrl && !event.shiftKey && event.key.toLowerCase() === "n") {
        event.preventDefault();
        createCard();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [screen, createCard]);

  /** Ctrl+Shift+C: весь код открытой карточки одним куском. */
  async function copyOpenedCardCode() {
    const data = card.data;
    if (data == null) {
      return;
    }
    const text = data.blocks.map((block) => block.code).join("\n\n");
    if (text.trim() === "") {
      return;
    }
    try {
      const { copyText } = await import("./utils/clipboard");
      await copyText(text);
      setCopyNotice("Код карточки скопирован");
      window.setTimeout(() => setCopyNotice(null), 2000);
    } catch {
      setCopyNotice("Не удалось скопировать код");
    }
  }

  const [copyNotice, setCopyNotice] = useState<string | null>(null);

  return (
    <Layout style={{ height: "100vh" }}>
      <Layout.Header
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 16,
          paddingInline: 24,
        }}
      >
        <Space size={16} align="center">
          <Typography.Title level={4} style={{ margin: 0 }}>
            Code Cards
          </Typography.Title>
          <Typography.Text type="secondary">
            Локальная библиотека фрагментов кода
          </Typography.Text>
        </Space>
        <Space size={12}>
          {copyNotice !== null && (
            <Typography.Text type="secondary">{copyNotice}</Typography.Text>
          )}
          <Tooltip title="Настройки, тема, копии базы">
            <Button
              icon={<SettingOutlined />}
              aria-label="Настройки"
              onClick={() => setSettingsOpen(true)}
            >
              Настройки
            </Button>
          </Tooltip>
        </Space>
      </Layout.Header>
      {/* app-content включает вертикальную прокрутку: длинный редактор
          должен прокручиваться до слотов блока, а не обрезаться окном. */}
      <Layout.Content className="app-content" style={{ padding: 24, minHeight: 0 }}>
        {screen.name === "list" && <CardList onOpen={openCard} onCreate={createCard} />}

        {screen.name === "view" &&
          (card.isError ? (
            <Flex vertical gap={16}>
              <Alert
                type="error"
                showIcon
                message="Не удалось открыть карточку"
                description={errorText(card.error)}
              />
              <a onClick={goList}>Вернуться к списку</a>
            </Flex>
          ) : (
            <CardView
              card={card.data ?? emptyFallback()}
              loading={card.isPending}
              onEdit={() => editCard(screen.id)}
              onBack={goList}
              onDeleted={goList}
            />
          ))}

        {screen.name === "edit" &&
          (card.isError ? (
            <Flex vertical gap={16}>
              <Alert
                type="error"
                showIcon
                message="Не удалось открыть карточку для правки"
                description={errorText(card.error)}
              />
              <a onClick={goList}>Вернуться к списку</a>
            </Flex>
          ) : editing === null ? (
            <Alert
              type="info"
              showIcon
              message="Карточка загружается"
              description={
                <a onClick={goList}>Если карточка не открывается, вернитесь к списку</a>
              }
            />
          ) : (
            <CardEditor
              // key пересоздаёт редактор при переходе к другой карточке:
              // состояние формы инициализируется из card только при монтировании.
              key={editing.id ?? "new"}
              card={editing}
              onClose={goList}
              onSaved={savedCard}
              registerHotkeys={(handlers) => {
                saveHandlerRef.current = handlers.onSave;
                cancelHandlerRef.current = handlers.onCancel;
              }}
            />
          ))}
      </Layout.Content>

      <Settings
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        themeMode={themeMode}
        onThemeModeChange={onThemeModeChange}
      />
    </Layout>
  );
}

/** Строка считается текстовым полем, если в ней вводят текст. */
function isTextInput(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  const tag = target.tagName.toLowerCase();
  if (tag === "textarea" || tag === "select") {
    return true;
  }
  if (tag === "input") {
    const type = (target as HTMLInputElement).type;
    return !["checkbox", "radio", "button", "submit"].includes(type);
  }
  return target.isContentEditable;
}

/** Пустая карточка: нужна только на время загрузки, чтобы не держать null в CardView. */
function emptyFallback(): CardDetail {
  return emptyCard();
}
