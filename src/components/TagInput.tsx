import { useMemo, useState } from "react";
import { AutoComplete, Alert, Button, Space, Tag, Typography } from "antd";
import { PlusOutlined } from "@ant-design/icons";
import type { TagWithCount } from "../api/tauri";
import { findSimilarTag } from "../hooks/useTags";

// Ввод тегов (раздел 3.3 плана): чипы, автодополнение из существующих тегов,
// выбор клавиатурой, мягкое предупреждение о похожем теге.
// Предупреждение не блокирует сохранение: тег можно оставить как введено.

export interface TagInputProps {
  /** Выбранные теги в порядке ввода. */
  value: string[];
  onChange: (tags: string[]) => void;
  /** Существующие теги для автодополнения. */
  known: TagWithCount[];
  /** true - только чтение. */
  disabled?: boolean;
  placeholder?: string;
}

/** Приводит ввод к виду, в котором его сохранит Rust: пробелы схлопнуты. */
function normalize(raw: string): string {
  return raw.split(/\s+/).filter(Boolean).join(" ");
}

/** Сравнение тегов без учёта регистра: база тоже дедуплицирует так. */
function sameTag(first: string, second: string): boolean {
  return first.toLowerCase() === second.toLowerCase();
}

export default function TagInput({
  value,
  onChange,
  known,
  disabled = false,
  placeholder = "Введите тег и нажмите Enter",
}: TagInputProps) {
  const [input, setInput] = useState("");
  const [hint, setHint] = useState<{ input: string; existing: string } | null>(null);

  const options = useMemo(() => {
    const query = input.trim().toLowerCase();
    return known
      .filter((tag) => !value.some((chosen) => sameTag(chosen, tag.name)))
      .filter((tag) => (query === "" ? true : tag.name.toLowerCase().includes(query)))
      .map((tag) => ({
        value: tag.name,
        label: (
          <Space size={6}>
            <span>{tag.name}</span>
            <Typography.Text type="secondary" className="count-chip">
              {tag.count}
            </Typography.Text>
          </Space>
        ),
      }));
  }, [known, input, value]);

  function addTag(raw: string) {
    const tag = normalize(raw);
    if (tag === "") {
      return;
    }
    if (value.some((chosen) => sameTag(chosen, tag))) {
      setInput("");
      setHint(null);
      return;
    }
    onChange([...value, tag]);
    setInput("");
    setHint(null);
  }

  function removeTag(tag: string) {
    onChange(value.filter((chosen) => chosen !== tag));
  }

  function handleTyping(raw: string) {
    setInput(raw);
    // Похожий тег ищем на каждый ввод, но не превращаем подсказку в блокировку.
    setHint(findSimilarTag(raw, known));
  }

  /** Заменяет ввод существующим тегом: подсказка "использовать его". */
  function replaceWithExisting() {
    if (hint === null) {
      return;
    }
    const target = hint.existing;
    setInput("");
    setHint(null);
    if (value.some((chosen) => sameTag(chosen, target))) {
      return;
    }
    onChange([...value, known.find((tag) => tag.name === target)?.name ?? target]);
  }

  function handleEnter() {
    // Enter всегда добавляет введённое: похожий тег только предлагается,
    // сохранение не блокируется.
    addTag(input);
  }

  return (
    <div>
      <Space wrap size={4} style={{ marginBottom: value.length > 0 ? 6 : 0 }}>
        {value.map((tag) => (
          <Tag key={tag} closable={!disabled} onClose={() => removeTag(tag)}>
            {tag}
          </Tag>
        ))}
      </Space>
      {!disabled && (
        <Space.Compact style={{ width: "100%" }}>
          <AutoComplete
            value={input}
            options={options}
            onChange={handleTyping}
            onSelect={(selected: string) => addTag(selected)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                handleEnter();
              }
              if (event.key === "Backspace" && input === "" && value.length > 0) {
                onChange(value.slice(0, -1));
              }
            }}
            style={{ width: "100%" }}
            placeholder={placeholder}
            allowClear
          />
          <Button icon={<PlusOutlined />} onClick={() => addTag(input)} disabled={input.trim() === ""}>
            Добавить
          </Button>
        </Space.Compact>
      )}
      {hint !== null && !disabled && (
        <Alert
          style={{ marginTop: 6 }}
          type="warning"
          showIcon
          message={
            <Space wrap size={6}>
              <span>
                Есть похожий тег <b>{hint.existing}</b>. Использовать его?
              </span>
              <Button size="small" onClick={replaceWithExisting}>
                Заменить на {hint.existing}
              </Button>
              <Button size="small" type="text" onClick={() => setHint(null)}>
                Оставить как есть
              </Button>
            </Space>
          }
        />
      )}
    </div>
  );
}
