import { App, Button, Card, Empty, Flex, Input, Space, Tag, Tooltip, Typography } from "antd";
import { DeleteOutlined, PlusOutlined } from "@ant-design/icons";
import type { Slot } from "../api/tauri";

// Редактор слотов блока (раздел 3.3 плана, этап 6):
// имя слота, значение по умолчанию и варианты значений (список строк).

export interface SlotsEditorProps {
  slots: Slot[];
  onChange: (slots: Slot[]) => void;
  /** Текст карточки в режиме "не слоты": редактор слотов не нужен. */
  disabled?: boolean;
}

export default function SlotsEditor({ slots, onChange, disabled = false }: SlotsEditorProps) {
  const { message } = App.useApp();

  function updateSlot(index: number, patch: Partial<Slot>) {
    onChange(slots.map((slot, position) => (position === index ? { ...slot, ...patch } : slot)));
  }

  function addSlot() {
    const name = `слот_${slots.length + 1}`;
    if (slots.some((slot) => slot.name === name)) {
      message.warning("Слот с таким именем уже есть");
      return;
    }
    onChange([...slots, { id: null, name, defaultValue: "", variants: [] }]);
  }

  function removeSlot(index: number) {
    onChange(slots.filter((_slot, position) => position !== index));
  }

  /** Варианты хранятся одной строкой на вариант: так проще вводить. */
  function setVariants(index: number, raw: string) {
    const variants = raw
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line !== "");
    updateSlot(index, { variants });
  }

  if (slots.length === 0) {
    return (
      <Empty
        image={Empty.PRESENTED_IMAGE_SIMPLE}
        description="Слотов пока нет: добавьте слот или сделайте чертёж из выделенного"
      >
        <Button size="small" icon={<PlusOutlined />} onClick={addSlot} disabled={disabled}>
          Добавить слот
        </Button>
      </Empty>
    );
  }

  return (
    <Flex vertical gap={8}>
      {slots.map((slot, index) => (
        <Card key={`${slot.name}-${index}`} size="small" bodyStyle={{ padding: 8 }}>
          <Flex vertical gap={6}>
            <Space wrap>
              <Input
                size="small"
                value={slot.name}
                disabled={disabled}
                onChange={(event) => updateSlot(index, { name: event.target.value.trim() })}
                addonBefore="{{"
                addonAfter="}}"
                style={{ width: 200 }}
                aria-label="Имя слота"
              />
              <Input
                size="small"
                value={slot.defaultValue}
                disabled={disabled}
                placeholder="Значение по умолчанию"
                onChange={(event) => updateSlot(index, { defaultValue: event.target.value })}
                style={{ width: 240 }}
              />
              <Tooltip title="Удалить слот">
                <Button
                  size="small"
                  type="text"
                  danger
                  icon={<DeleteOutlined />}
                  disabled={disabled}
                  onClick={() => removeSlot(index)}
                />
              </Tooltip>
              {slot.variants.length > 0 && <Tag color="gold">{`вариантов: ${slot.variants.length}`}</Tag>}
            </Space>
            <Input.TextArea
              size="small"
              value={slot.variants.join("\n")}
              disabled={disabled}
              placeholder="Варианты значений, по одному в строке (необязательно)"
              onChange={(event) => setVariants(index, event.target.value)}
              autoSize={{ minRows: 1, maxRows: 4 }}
            />
          </Flex>
        </Card>
      ))}
      <Space>
        <Button size="small" icon={<PlusOutlined />} onClick={addSlot} disabled={disabled}>
          Добавить слот
        </Button>
        <Typography.Text type="secondary">
          Имена слотов должны встречаться в коде как {"{{имя}}"}.
        </Typography.Text>
      </Space>
    </Flex>
  );
}
