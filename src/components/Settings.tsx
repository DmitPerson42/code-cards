import { useState } from "react";
import {
  App,
  Button,
  Card,
  Descriptions,
  Flex,
  Modal,
  Segmented,
  Space,
  Typography,
} from "antd";
import {
  CloudDownloadOutlined,
  CloudUploadOutlined,
  DatabaseOutlined,
} from "@ant-design/icons";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  backupExportJson,
  errorText,
  backupImportJson,
  backupRun,
  type ImportReport,
} from "../api/tauri";
import { useQueryClient } from "@tanstack/react-query";
import type { ThemeMode } from "../hooks/useTheme";

// Настройки приложения (этап 7): тема, резервная копия базы,
// экспорт и импорт архива. Все файлы выбираются диалогами Tauri,
// сетевых запросов приложение не делает.

export interface SettingsProps {
  open: boolean;
  onClose: () => void;
  themeMode: ThemeMode;
  onThemeModeChange: (mode: ThemeMode) => void;
}

/** Отчёт об импорте в человеческом виде. */
function reportLines(report: ImportReport): string[] {
  return [
    `карточек: ${report.cards}`,
    `блоков кода: ${report.blocks}`,
    `записей разбора: ${report.notes}`,
    `слотов: ${report.slots}`,
    `тегов: ${report.tags}`,
    `подборок: ${report.collections}`,
    `создано заново из-за совпадения id: ${report.recreated}`,
  ];
}

export default function Settings({
  open: visible,
  onClose,
  themeMode,
  onThemeModeChange,
}: SettingsProps) {
  const { message, modal } = App.useApp();
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState<"none" | "backup" | "export" | "import">("none");

  /** После импорта и копии базы данные меняются целиком: сбрасываем кэш запросов. */
  function refreshAll() {
    void queryClient.invalidateQueries();
  }

  async function makeBackup() {
    setBusy("backup");
    try {
      const path = await backupRun();
      message.success(`Копия базы создана: ${path}`);
    } catch (error) {
      message.error(
        `Не удалось сделать копию: ${errorText(error)}`,
      );
    } finally {
      setBusy("none");
    }
  }

  async function exportJson() {
    const path = await save({
      title: "Экспорт архива в JSON",
      defaultPath: "code-cards-archive.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (path == null) {
      return;
    }
    setBusy("export");
    try {
      const stats = await backupExportJson(path);
      message.success(
        `Экспортировано: карточек ${stats.cards}, блоков ${stats.blocks}, тегов ${stats.tags}`,
      );
    } catch (error) {
      message.error(
        `Не удалось экспортировать: ${errorText(error)}`,
      );
    } finally {
      setBusy("none");
    }
  }

  async function importJson() {
    const path = await open({
      title: "Импорт архива из JSON",
      multiple: false,
      directory: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (path == null || typeof path !== "string") {
      return;
    }
    setBusy("import");
    try {
      const report = await backupImportJson(path);
      refreshAll();
      // Отчёт обязателен: импорт никогда не перезаписывает старые карточки.
      modal.success({
        title: "Импорт завершён",
        width: 420,
        content: (
          <Flex vertical gap={2}>
            {reportLines(report).map((line) => (
              <span key={line}>{line}</span>
            ))}
            <Typography.Text type="secondary">
              Существующие карточки не изменялись: при совпадении id созданы новые.
            </Typography.Text>
          </Flex>
        ),
      });
    } catch (error) {
      message.error(
        `Не удалось импортировать: ${errorText(error)}`,
      );
    } finally {
      setBusy("none");
    }
  }

  return (
    <Modal
      open={visible}
      onCancel={onClose}
      footer={
        <Button onClick={onClose}>Закрыть</Button>
      }
      title="Настройки"
      width={620}
    >
      <Flex vertical gap={16}>
        <Card size="small" title="Тема оформления">
          <Space direction="vertical" size={4}>
            <Segmented
              value={themeMode}
              onChange={(value) => onThemeModeChange(value as ThemeMode)}
              options={[
                { value: "dark", label: "Тёмная" },
                { value: "light", label: "Светлая" },
              ]}
            />
            <Typography.Text type="secondary">
              Выбор сохраняется и применяется при следующем запуске.
            </Typography.Text>
          </Space>
        </Card>

        <Card size="small" title="Данные">
          <Flex vertical gap={10}>
            <Space wrap>
              <Button
                icon={<DatabaseOutlined />}
                loading={busy === "backup"}
                onClick={() => void makeBackup()}
              >
                Сделать копию базы
              </Button>
              <Button
                icon={<CloudUploadOutlined />}
                loading={busy === "export"}
                onClick={() => void exportJson()}
              >
                Экспорт в JSON
              </Button>
              <Button
                icon={<CloudDownloadOutlined />}
                loading={busy === "import"}
                onClick={() => void importJson()}
              >
                Импорт из JSON
              </Button>
            </Space>
            <Descriptions size="small" column={1} bordered={false}>
              <Descriptions.Item label="Копии базы">
                Копия базы делается автоматически при запуске, если прошло больше суток.
                Хранится 10 последних.
              </Descriptions.Item>
              <Descriptions.Item label="Импорт">
                Ничего не перезаписывает: при совпадении id создаются новые карточки.
              </Descriptions.Item>
              <Descriptions.Item label="Сеть">
                Приложение работает полностью офлайн, никаких сетевых запросов нет.
              </Descriptions.Item>
            </Descriptions>
          </Flex>
        </Card>
      </Flex>
    </Modal>
  );
}
