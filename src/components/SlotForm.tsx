import { useEffect, useMemo, useState } from "react";
import { App, Button, Card, Empty, Flex, Input, Select, Space, Tag, Typography } from "antd";
import { CopyOutlined, CheckOutlined } from "@ant-design/icons";
import { errorText, renderCard, type CardDetail, type Slot } from "../api/tauri";
import { copyText } from "../utils/clipboard";
import CodeBlockView from "./CodeBlockView";

// Форма заполнения слотов (раздел 3.3 плана, этап 6):
// поля по слотам, умолчания подставлены, при наличии вариантов вместо
// текстового поля выпадающий список. Результат показывается отдельной панелью.
// Карточка без слотов или в режиме "не слоты" форму не показывает.

export interface SlotFormProps {
  card: CardDetail;
  /** Закрыть форму (например, после "Скопировать и закрыть"). */
  onClose: () => void;
}

/** Все слоты карточки в порядке блоков, без дублей имён. */
export function collectSlots(card: CardDetail): Slot[] {
  const result: Slot[] = [];
  for (const block of card.blocks) {
    for (const slot of block.slots) {
      if (!result.some((known) => known.name === slot.name)) {
        result.push(slot);
      }
    }
  }
  return result;
}

/** Показывать ли форму: нужны режим шаблона и хотя бы один слот. */
export function hasFillableSlots(card: CardDetail): boolean {
  return card.isTemplate && card.slotSyntax !== "jinja_off" && collectSlots(card).length > 0;
}

export default function SlotForm({ card, onClose }: SlotFormProps) {
  const { message } = App.useApp();
  const slots = useMemo(() => collectSlots(card), [card]);
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(slots.map((slot) => [slot.name, slot.defaultValue])),
  );
  const [result, setResult] = useState<string[] | null>(null);
  const [rendering, setRendering] = useState(false);
  const [copied, setCopied] = useState(false);

  // Значения по умолчанию меняются при смене карточки.
  useEffect(() => {
    setValues(Object.fromEntries(slots.map((slot) => [slot.name, slot.defaultValue])));
    setResult(null);
    setCopied(false);
  }, [slots]);

  // Подстановку считает Rust: фронтенд не знает правил синтаксиса слотов.
  useEffect(() => {
    let cancelled = false;
    setRendering(true);
    renderCard(card.id as number, values)
      .then((blocks) => {
        if (!cancelled) {
          setResult(blocks);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setResult(null);
          message.error(
            `Не удалось подставить слоты: ${errorText(error)}`,
          );
        }
      })
      .finally(() => {
        if (!cancelled) {
          setRendering(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [card.id, values, message]);

  if (card.id == null) {
    return null;
  }

  async function copyResult(closeAfter: boolean) {
    if (result === null) {
      return;
    }
    const text = result.join("\n\n");
    try {
      await copyText(text);
      setCopied(true);
      message.success("Результат скопирован");
      if (closeAfter) {
        onClose();
      }
    } catch (error) {
      message.error(errorText(error));
    }
  }

  return (
    <Card
      size="small"
      title="Заполнение слотов"
      extra={
        <Space>
          <Button icon={<CopyOutlined />} onClick={() => void copyResult(false)}>
            {copied ? "Скопировано" : "Копировать"}
          </Button>
          <Button type="primary" icon={<CheckOutlined />} onClick={() => void copyResult(true)}>
            Скопировать и закрыть
          </Button>
        </Space>
      }
    >
      <Flex vertical gap={12}>
        {slots.length === 0 && <Empty description="У карточки нет слотов" />}

        {slots.map((slot) => (
          <Flex key={slot.name} vertical gap={2}>
            <Tag color="gold" style={{ width: "fit-content" }}>{`{{${slot.name}}}`}</Tag>
            {slot.variants.length > 0 ? (
              <Select
                value={values[slot.name] ?? ""}
                onChange={(next: string) =>
                  setValues((current) => ({ ...current, [slot.name]: next }))
                }
                options={slot.variants.map((variant) => ({ value: variant, label: variant }))}
                placeholder="Выберите значение"
                style={{ maxWidth: 360 }}
              />
            ) : (
              <Input
                value={values[slot.name] ?? ""}
                onChange={(event) =>
                  setValues((current) => ({ ...current, [slot.name]: event.target.value }))
                }
                style={{ maxWidth: 360 }}
              />
            )}
          </Flex>
        ))}

        <Typography.Text strong>Результат</Typography.Text>
        {rendering && <Typography.Text type="secondary">Подстановка...</Typography.Text>}
        {result !== null &&
          result.map((code, index) => (
            <div
              key={index}
              style={{
                border: "1px solid rgba(255,255,255,0.12)",
                borderRadius: 6,
                maxHeight: 320,
                overflow: "auto",
              }}
            >
              <CodeBlockView
                value={code}
                language={card.blocks[index]?.language ?? "text"}
                editable={false}
              />
            </div>
          ))}
      </Flex>
    </Card>
  );
}
