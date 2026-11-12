import { useEffect, useMemo, useRef, useState } from "react";
import {
  App,
  Button,
  Card,
  Checkbox,
  Empty,
  Flex,
  Input,
  Segmented,
  Select,
  Space,
  Tabs,
  Tag,
  Tooltip,
  Typography,
} from "antd";
import {
  CopyOutlined,
  DeleteOutlined,
  HighlightOutlined,
  InfoOutlined,
  PlusOutlined,
  SaveOutlined,
} from "@ant-design/icons";
import {
  blockSlotsScan,
  errorText,
  makeSlotCommand,
  SLOT_DOUBLE_BRACE,
  type CardDetail,
  type CodeBlock,
  type FoundSlot,
  type MovedSummary,
  type SaveResult,
  type Slot,
  type SlotSyntax,
} from "../api/tauri";
import { useSaveCard } from "../hooks/useCards";
import { toDraft, useNotes } from "../hooks/useNotes";
import type { DraftNote } from "../hooks/useNotes";
import { useTags } from "../hooks/useTags";
import { copyText } from "../utils/clipboard";
import CodeBlockView, { CODE_LANGUAGES, type SelectionInfo } from "./CodeBlockView";
import NotesPanel from "./NotesPanel";
import SlotsEditor from "./SlotsEditor";
import TagInput from "./TagInput";
import { newBlockKey, toBlockDrafts, type BlockDraft } from "./blockDraft";

// Редактор карточки (этапы 1-3): название, описание, теги, блоки кода, разбор по строкам.
// Слева код с подсветкой разобранных строк, справа панель разбора (раздел 3.3).
// Слоты и чертежи подключаются на этапе 6.
// Сводка переноса разбора обязательна: молча терять разбор запрещено (правило 5 плана).

export const DEFAULT_LANGUAGE = "python";

/** Новая пустая карточка для кнопки "Новая карточка". */
export function emptyCard(): CardDetail {
  return {
    id: null,
    title: "",
    description: "",
    isTemplate: false,
    slotSyntax: SLOT_DOUBLE_BRACE,
    favorite: false,
    createdAt: "",
    updatedAt: "",
    checkedVersion: "",
    checkedAt: "",
    tags: [],
    blocks: [emptyBlock(0)],
  };
}

function emptyBlock(sortOrder: number): CodeBlock {
  return {
    id: null,
    language: DEFAULT_LANGUAGE,
    code: "",
    sortOrder,
    notes: [],
    slots: [],
  };
}

/** Набор действий редактора, которые вызывает App по горячим клавишам. */
export interface EditorHotkeys {
  onSave: () => void;
  onCancel: () => void;
}

export interface CardEditorProps {
  /** Исходная карточка; меняется только копией в состоянии редактора. */
  card: CardDetail;
  /** Открыть список после сохранения или отказа. */
  onClose: () => void;
  /** После сохранения открыть карточку по id, который вернул card_save. */
  onSaved: (id: number) => void;
  /** Регистрация обработчиков Ctrl+S и Esc для родителя. */
  registerHotkeys?: (handlers: EditorHotkeys) => void;
}

export default function CardEditor({
  card,
  onClose,
  onSaved,
  registerHotkeys,
}: CardEditorProps) {
  const { message, modal } = App.useApp();
  const saveCard = useSaveCard();
  const knownTags = useTags();

  const [title, setTitle] = useState(card.title);
  const [description, setDescription] = useState(card.description);
  const [tags, setTags] = useState<string[]>(card.tags);
  // Галочка "использует слоты" и режим синтаксиса слотов.
  const [isTemplate, setIsTemplate] = useState(card.isTemplate);
  const [slotSyntax, setSlotSyntax] = useState<SlotSyntax>(card.slotSyntax);
  const [blocks, setBlocks] = useState<BlockDraft[]>(() => buildDrafts(card));
  // Черновик в ref: отложенные обработчики окна сводки читают актуальные данные.
  const blocksRef = useMemo(() => ({ current: blocks }), []);
  blocksRef.current = blocks;
  const [activeKey, setActiveKey] = useState<string>(() => (blocks[0]?.key ?? ""));

  // Правка кода блока: меняется только код, разбор не трогаем.
  function updateBlockCode(key: string, code: string) {
    setBlocks((current) =>
      current.map((block) => (block.key === key ? { ...block, code } : block)),
    );
  }

  function updateBlockLanguage(key: string, language: string) {
    setBlocks((current) =>
      current.map((block) => (block.key === key ? { ...block, language } : block)),
    );
  }

  function updateBlockNotes(key: string, notes: DraftNote[]) {
    setBlocks((current) =>
      current.map((block) => (block.key === key ? { ...block, notes } : block)),
    );
  }

  function updateBlockSlots(key: string, slots: Slot[]) {
    setBlocks((current) =>
      current.map((block) => (block.key === key ? { ...block, slots } : block)),
    );
  }

  function addBlock() {
    const created = buildDrafts({ ...card, blocks: [emptyBlock(blocks.length)] })[0];
    if (created === undefined) {
      return;
    }
    setBlocks((current) => [...current, created]);
    setActiveKey(created.key);
  }

  function removeBlock(key: string) {
    setBlocks((current) => {
      const next = current.filter((block) => block.key !== key);
      return next.length > 0 ? next : buildDrafts({ ...card, blocks: [emptyBlock(0)] });
    });
    setActiveKey((current) => (current === key ? "" : current));
  }

  async function copyBlockCode(key: string) {
    const block = blocks.find((item) => item.key === key);
    if (block === undefined || block.code === "") {
      message.warning("Блок кода пуст: копировать нечего");
      return;
    }
    try {
      await copyText(block.code);
      message.success("Код скопирован");
    } catch (error) {
      message.error(errorText(error));
    }
  }

  /** Собирает DTO для card_save: порядок блоков и разбора выравнивается по индексу. */
  function buildDetail(draft: BlockDraft[]): CardDetail {
    return {
      ...card,
      title: title.trim(),
      description,
      tags,
      isTemplate,
      slotSyntax,
      blocks: draft.map((block, index) => ({
        id: block.id,
        language: block.language,
        code: block.code,
        sortOrder: index,
        notes: block.notes.map((note, noteIndex) => ({
          id: note.id ?? null,
          lineFrom: note.lineFrom,
          lineTo: note.lineTo,
          text: note.text,
          isStale: note.isStale,
          sortOrder: noteIndex,
        })),
        slots: block.slots,
      })),
    };
  }

  function handleResult(result: SaveResult) {
    if (result.status === "ok") {
      message.success("Карточка сохранена");
      onSaved(result.id);
      return;
    }
    askConfirmation(result.summary);
  }

  /** Сводка переноса разбора: без подтверждения сохранение не выполняется. */
  function askConfirmation(summary: MovedSummary) {
    modal.confirm({
      title: "Разбор будет перенесён",
      icon: <InfoOutlined />,
      width: 460,
      content: (
        <Space direction="vertical" size={6}>
          <span>Сдвинуто диапазонов: {summary.moved}</span>
          <span>Изменено: {summary.changed}</span>
          <span>Потеряно: {summary.lost}</span>
          <Typography.Text type="secondary">
            Потерянные диапазоны не удаляются, они помечаются устаревшими.
          </Typography.Text>
        </Space>
      ),
      okText: "Сохранить",
      cancelText: "Отменить",
      onOk: () =>
        new Promise<void>((resolve, reject) => {
          // Повторный вызов с force = true: только он пишет в базу.
          saveCard.mutate(
            { detail: buildDetail(blocksRef.current), force: true },
            {
              onSuccess: (forced) => {
                if (forced.status === "ok") {
                  message.success("Карточка сохранена");
                  onSaved(forced.id);
                  resolve();
                  return;
                }
                // Сводка пришла снова: не подтверждаем молча, показываем снова.
                askConfirmation(forced.summary);
                resolve();
              },
              onError: (error) => {
                message.error(`Не удалось сохранить: ${errorText(error)}`);
                reject(error);
              },
            },
          );
        }),
    });
  }

  function handleSave() {
    if (title.trim() === "") {
      message.warning("Укажите название карточки");
      return;
    }
    // Код изменён, а разбор есть: предупреждаем заранее, что диапазоны
    // могут сдвинуться. Сводка от Rust приходит следующим шагом.
    const edited = blocks.filter((block) => block.notes.length > 0);
    const original = buildDrafts(card);
    const changedWithNotes = edited.some((block) => {
      const source = original.find((item) => item.id != null && item.id === block.id);
      return source !== undefined && source.code !== block.code;
    });
    if (changedWithNotes) {
      modal.confirm({
        title: "Код изменён, разбор привязан к строкам",
        icon: <InfoOutlined />,
        content:
          "Диапазоны разбора будут перенесены на новые строки, а изменённые пометятся устаревшими. Перед сохранением будет показана сводка.",
        okText: "Проверить и сохранить",
        cancelText: "Отменить",
        onOk: () => performSave(),
      });
      return;
    }
    performSave();
  }

  /** Отправляет карточку в Rust и обрабатывает сводку переноса. */
  function performSave() {
    saveCard.mutate(
      { detail: buildDetail(blocks) },
      {
        onSuccess: handleResult,
        onError: (error) => message.error(`Не удалось сохранить: ${errorText(error)}`),
      },
    );
  }

  function handleCancel() {
    if (!dirty) {
      onClose();
      return;
    }
    modal.confirm({
      title: "Закрыть без сохранения?",
      content: "Изменения будут потеряны.",
      okText: "Закрыть без сохранения",
      cancelText: "Продолжить правку",
      onOk: onClose,
    });
  }

  // Горячие клавиши Ctrl+S и Esc живут в App: редактор отдаёт ему свои действия.
  const hotkeysRef = useRef({ onSave: () => {}, onCancel: () => {} });
  hotkeysRef.current = {
    onSave: () => handleSave(),
    onCancel: () => handleCancel(),
  };
  useEffect(() => {
    registerHotkeys?.({
      onSave: () => hotkeysRef.current.onSave(),
      onCancel: () => hotkeysRef.current.onCancel(),
    });
    // Регистрация одна на открытие редактора: действия берутся из ref.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [registerHotkeys]);

  const dirty = useMemo(() => {    if (title !== card.title || description !== card.description) {
      return true;
    }
    if (
      tags.length !== card.tags.length ||
      tags.some((tag, index) => tag !== card.tags[index])
    ) {
      return true;
    }
    if (isTemplate !== card.isTemplate || slotSyntax !== card.slotSyntax) {
      return true;
    }
    const original = buildDrafts(card);
    if (blocks.length !== original.length) {
      return true;
    }
    return blocks.some((block, index) => {
      const source = original[index];
      if (source === undefined) {
        return true;
      }
      if (block.code !== source.code || block.language !== source.language) {
        return true;
      }
      if (JSON.stringify(block.slots) !== JSON.stringify(source.slots)) {
        return true;
      }
      if (block.notes.length !== source.notes.length) {
        return true;
      }
      return block.notes.some((note, noteIndex) => {
        const originalNote = source.notes[noteIndex];
        return (
          originalNote === undefined ||
          note.lineFrom !== originalNote.lineFrom ||
          note.lineTo !== originalNote.lineTo ||
          note.text !== originalNote.text ||
          note.isStale !== originalNote.isStale
        );
      });
    });
  }, [title, description, tags, isTemplate, slotSyntax, blocks, card]);

  return (
    <Flex vertical gap={12} style={{ height: "100%", minHeight: 0 }}>
      <Space wrap style={{ justifyContent: "space-between", width: "100%" }}>
        <Typography.Title level={4} style={{ margin: 0 }}>
          {card.id == null ? "Новая карточка" : "Редактирование карточки"}
        </Typography.Title>
        <Space>
          <Button icon={<CopyOutlined />} onClick={() => void copyBlockCode(activeKey)}>
            Копировать код
          </Button>
          <Button onClick={handleCancel}>Отмена</Button>
          <Button
            type="primary"
            icon={<SaveOutlined />}
            loading={saveCard.isPending}
            onClick={handleSave}
          >
            Сохранить
          </Button>
        </Space>
      </Space>

      <Card size="small" title="Название, описание и теги">
        <Flex vertical gap={12}>
          <Input
            placeholder="Название карточки"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            status={title.trim() === "" ? "warning" : undefined}
            autoFocus
          />
          <Input.TextArea
            placeholder="Что делает код в целом"
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            autoSize={{ minRows: 2, maxRows: 4 }}
          />
          <div>
            <Typography.Text type="secondary" style={{ display: "block", marginBottom: 4 }}>
              Теги
            </Typography.Text>
            <TagInput
              value={tags}
              onChange={setTags}
              known={knownTags.data ?? []}
            />
          </div>
          <Space wrap size="middle">
            <Checkbox
              checked={isTemplate}
              onChange={(event) => setIsTemplate(event.target.checked)}
            >
              Использует слоты
            </Checkbox>
            <span>Синтаксис:</span>
            <Segmented
              value={slotSyntax}
              onChange={(value) => setSlotSyntax(value as SlotSyntax)}
              options={[
                { value: "double_brace", label: "{{имя}}" },
                { value: "jinja_off", label: "не слоты" },
              ]}
            />
            <Typography.Text type="secondary">
              В режиме «не слоты» фигурные скобки в коде не считаются шаблоном.
            </Typography.Text>
          </Space>
        </Flex>
      </Card>

      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <Tabs
          activeKey={activeKey}
          onChange={setActiveKey}
          type="editable-card"
          onEdit={(targetKey, action) => {
            if (action === "remove" && typeof targetKey === "string") {
              removeBlock(targetKey);
            }
          }}
          tabBarExtraContent={
            <Space>
              <Button icon={<PlusOutlined />} onClick={addBlock}>
                Ещё блок
              </Button>
            </Space>
          }
          items={blocks.map((block, index) => ({
            key: block.key,
            label: `Блок ${index + 1}`,
            closable: blocks.length > 1,
            children: (
              <BlockPane
                key={block.key}
                block={block}
                onCodeChange={(code) => updateBlockCode(block.key, code)}
                onLanguageChange={(language) => updateBlockLanguage(block.key, language)}
                onNotesChange={(notes) => updateBlockNotes(block.key, notes)}
                onSlotsChange={(slots) => updateBlockSlots(block.key, slots)}
                slotsEnabled={isTemplate && slotSyntax !== "jinja_off"}
                slotSyntax={slotSyntax}
                onSaveShortcut={handleSave}
                onCopy={() => void copyBlockCode(block.key)}
                onRemove={() => removeBlock(block.key)}
                canRemove={blocks.length > 1}
              />
            ),
          }))}
        />
        {blocks.length === 0 && <Empty description="Нет блоков кода" />}
      </div>
    </Flex>
  );
}

/** Черновики блоков из карточки: ключи новые на каждое открытие. */
function buildDrafts(card: CardDetail): BlockDraft[] {
  const source = card.blocks.length > 0 ? card.blocks : [emptyBlock(0)];
  return toBlockDrafts(source, newBlockKey, (notes) => toDraft(notes));
}

interface BlockPaneProps {
  block: BlockDraft;
  onCodeChange: (code: string) => void;
  onLanguageChange: (language: string) => void;
  onNotesChange: (notes: DraftNote[]) => void;
  onSlotsChange: (slots: Slot[]) => void;
  /** Подсветка и редактор слотов включаются только при галочке и режиме {{ }}. */
  slotsEnabled: boolean;
  slotSyntax: SlotSyntax;
  onSaveShortcut: () => void;
  onCopy: () => void;
  onRemove: () => void;
  canRemove: boolean;
}

/**
 * Один блок в редакторе: код слева, разбор справа.
 * Разбор живёт в хуке useNotes, а изменения отправляются наверх целиком.
 */
function BlockPane({
  block,
  onCodeChange,
  onLanguageChange,
  onNotesChange,
  onSlotsChange,
  slotsEnabled,
  slotSyntax,
  onSaveShortcut,
  onCopy,
  onRemove,
  canRemove,
}: BlockPaneProps) {
  const { message, modal } = App.useApp();
  const notes = useNotes(block.notes);
  // Любое изменение записей поднимается в родителя.
  useEffect(() => {
    onNotesChange(notes.notes);
    // Родитель обновляет тот же блок: функция подписки стабильна только по смыслу,
    // поэтому зависим от самих записей.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [notes.notes]);

  const [view, setView] = useState<"split" | "code">("split");
  // Найденные слоты появляются только при включённой галочке и режиме {{ }}.
  const [found, setFound] = useState<FoundSlot[]>([]);
  const [selection, setSelection] = useState<SelectionInfo | null>(null);

  // Сканирует код Rust: правила {{...}} живут в slots.rs, не в React.
  useEffect(() => {
    let cancelled = false;
    if (!slotsEnabled) {
      setFound([]);
      return;
    }
    blockSlotsScan(block.code, slotSyntax)
      .then((slots) => {
        if (!cancelled) {
          setFound(slots);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setFound([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [block.code, slotSyntax, slotsEnabled]);

  /** "Сделать чертёж из выделенного": выделение превращается в {{имя}} слота. */
  function makeSlotFromSelection() {
    if (selection === null) {
      message.warning("Сначала выделите фрагмент строки в коде");
      return;
    }
    let name = "";
    modal.confirm({
      title: "Сделать чертёж из выделенного",
      content: (
        <Input
          placeholder="Имя слота: буквы, цифры, подчёркивание, дефис"
          onChange={(event) => {
            name = event.target.value;
          }}
          autoFocus
        />
      ),
      okText: "Сделать слот",
      cancelText: "Отмена",
      onOk: () =>
        new Promise<void>((resolve, reject) => {
          makeSlotCommand(
            block.code,
            selection.line,
            selection.rangeStart,
            selection.rangeEnd,
            name.trim(),
          )
            .then(([newCode, defaultValue]) => {
              onCodeChange(newCode);
              const trimmed = name.trim();
              const existing = block.slots.find((slot) => slot.name === trimmed);
              if (existing === undefined) {
                onSlotsChange([
                  ...block.slots,
                  { id: null, name: trimmed, defaultValue, variants: [] },
                ]);
              }
              message.success(`Слот {{${trimmed}}} создан`);
              resolve();
            })
            .catch((error: unknown) => {
              // Rust отклоняет пустое выделение и недопустимое имя: текст приходит строкой.
              message.error(errorText(error));
              reject(error);
            });
        }),
    });
  }

  return (
    <Flex vertical gap={8} style={{ minHeight: 0 }}>
      <Space wrap>
        <span>Язык:</span>
        <Select
          value={block.language}
          onChange={onLanguageChange}
          options={CODE_LANGUAGES.map((item) => ({ value: item.value, label: item.label }))}
          style={{ width: 160 }}
        />
        <Segmented
          value={view}
          onChange={(value) => setView(value as "split" | "code")}
          options={[
            { value: "split", label: "Код и разбор" },
            { value: "code", label: "Только код" },
          ]}
        />
        <Button icon={<CopyOutlined />} onClick={onCopy}>
          Копировать
        </Button>
        <Tooltip
          title={
            slotsEnabled
              ? "Выделите фрагмент строки и превратите его в слот"
              : "Сначала отметьте «Использует слоты»"
          }
        >
          <Button
            icon={<HighlightOutlined />}
            disabled={!slotsEnabled || selection === null}
            onClick={makeSlotFromSelection}
          >
            Сделать чертёж из выделенного
          </Button>
        </Tooltip>
        {slotsEnabled && <Tag color="gold">{`слотов в коде: ${found.length}`}</Tag>}
        <Button danger icon={<DeleteOutlined />} disabled={!canRemove} onClick={onRemove}>
          Удалить блок
        </Button>
      </Space>

      {/* Высота области кода - доля экрана, а не фиксированный вычет:
          раньше maxHeight обрезал блок и панель разбора, а всё, что ниже
          (слоты блока), уходило за границу окна. Прокрутку страницы
          обеспечивает .app-content. */}
      <div
        className={view === "split" ? "card-split" : undefined}
        style={{ maxHeight: "min(60vh, 720px)" }}
      >
        <div
          style={{
            border: "1px solid rgba(255,255,255,0.12)",
            borderRadius: 6,
            minHeight: 320,
            overflow: "auto",
          }}
        >
          <CodeBlockView
            value={block.code}
            language={block.language}
            editable
            onChange={onCodeChange}
            onSaveShortcut={onSaveShortcut}
            ranges={notes.ranges}
            activeKey={notes.selectedKey}
            focusLine={notes.focusLine}
            focusToken={notes.focusToken}
            onLineActivate={notes.activateLine}
            slots={slotsEnabled ? found : []}
            onSelectionChange={setSelection}
          />
        </div>
        {view === "split" && (
          <div style={{ minHeight: 320, overflow: "auto", paddingLeft: 4 }}>
            <NotesPanel
              notes={notes}
              code={block.code}
              editable
              title={`Разбор блока: ${block.code.split("\n").length} стр.`}
            />
          </div>
        )}
      </div>

      {slotsEnabled && (
        <Card size="small" title="Слоты блока">
          <SlotsEditor slots={block.slots} onChange={onSlotsChange} />
        </Card>
      )}
    </Flex>
  );
}
