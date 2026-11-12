import { useMemo, useState } from "react";
import {
  App,
  Button,
  Card,
  Flex,
  Popconfirm,
  Space,
  Spin,
  Tag,
  Typography,
} from "antd";
import {
  CopyOutlined,
  DeleteOutlined,
  EditOutlined,
  FileMarkdownOutlined,
  StarFilled,
  StarOutlined,
  ThunderboltOutlined,
} from "@ant-design/icons";
import { save } from "@tauri-apps/plugin-dialog";
import { backupExportMarkdown, errorText, type CardDetail, type CodeBlock } from "../api/tauri";
import { useDeleteCard, useToggleFavorite } from "../hooks/useCards";
import { useSetCardTags, useTags } from "../hooks/useTags";
import { toDraft, useNotes } from "../hooks/useNotes";
import { copyText } from "../utils/clipboard";
import CodeBlockView from "./CodeBlockView";
import NotesPanel from "./NotesPanel";
import TagInput from "./TagInput";
import AddToCollection from "./AddToCollection";
import SlotForm, { hasFillableSlots } from "./SlotForm";

// Просмотр карточки (этапы 1-3): код слева, разбор справа, двусторонняя подсветка,
// теги с точечным сохранением через card_set_tags.
// Слоты и заполнение шаблона подключаются на этапе 6.

export interface CardViewProps {
  card: CardDetail;
  /** Запрос карточки ещё идёт. */
  loading: boolean;
  onEdit: () => void;
  onBack: () => void;
  /** Вызывается после удаления карточки. */
  onDeleted: () => void;
}

export default function CardView({
  card,
  loading,
  onEdit,
  onBack,
  onDeleted,
}: CardViewProps) {
  const { message } = App.useApp();
  const deleteCard = useDeleteCard();
  const toggleFavorite = useToggleFavorite();
  const setCardTags = useSetCardTags();
  const knownTags = useTags();
  const [copiedBlock, setCopiedBlock] = useState<number | null>(null);
  const [editingTags, setEditingTags] = useState(false);
  const [tagDraft, setTagDraft] = useState<string[]>(card.tags);

  function startTagEditing() {
    setTagDraft(card.tags);
    setEditingTags(true);
  }

  function saveTags() {
    if (card.id == null) {
      return;
    }
    setCardTags.mutate(
      { cardId: card.id, tags: tagDraft },
      {
        onSuccess: () => {
          message.success("Теги сохранены");
          setEditingTags(false);
        },
        onError: (error) => message.error(`Не удалось сохранить теги: ${errorText(error)}`),
      },
    );
  }

  async function copyBlock(index: number, code: string) {
    try {
      await copyText(code);
      setCopiedBlock(index);
      message.success("Код скопирован");
      window.setTimeout(
        () => setCopiedBlock((current) => (current === index ? null : current)),
        1500,
      );
    } catch (error) {
      message.error(errorText(error));
    }
  }

  function handleDelete() {
    if (card.id == null) {
      return;
    }
    deleteCard.mutate(card.id, {
      onSuccess: () => {
        message.success("Карточка удалена");
        onDeleted();
      },
      onError: (error) => message.error(`Не удалось удалить: ${errorText(error)}`),
    });
  }

  const blocks = card.blocks;
  // Форма показывается только у шаблона с реальными слотами (не jinja_off).
  const showSlotForm = card.id != null && hasFillableSlots(card);
  const [fillingSlots, setFillingSlots] = useState(false);
  const [exporting, setExporting] = useState(false);

  /** Экспорт карточки в Markdown: код и таблица разбора, путь выбирает пользователь. */
  async function exportMarkdown() {
    if (card.id == null) {
      return;
    }
    const safeName = card.title.replace(/[\\/:*?"<>|]/g, "_").trim() || "card";
    const path = await save({
      title: "Экспорт карточки в Markdown",
      defaultPath: `${safeName}.md`,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (path == null) {
      return;
    }
    setExporting(true);
    try {
      await backupExportMarkdown(card.id, path);
      message.success(`Карточка экспортирована: ${path}`);
    } catch (error) {
      message.error(
        `Не удалось экспортировать: ${errorText(error)}`,
      );
    } finally {
      setExporting(false);
    }
  }

  return (
    <Flex vertical gap={12} style={{ height: "100%", minHeight: 0 }}>
      <Space wrap style={{ justifyContent: "space-between", width: "100%" }}>
        <Space>
          <Button onClick={onBack}>К списку</Button>
          <Typography.Title level={4} style={{ margin: 0 }}>
            {card.title}
          </Typography.Title>
          {card.id != null && (
            <Button
              type="text"
              icon={
                card.favorite ? (
                  <StarFilled style={{ color: "#f0b429" }} />
                ) : (
                  <StarOutlined />
                )
              }
              loading={toggleFavorite.isPending}
              onClick={() => toggleFavorite.mutate(card.id as number)}
            >
              {card.favorite ? "В избранном" : "В избранное"}
            </Button>
          )}
        </Space>
        <Space>
          {card.id != null && <AddToCollection cardId={card.id} />}
          {showSlotForm && (
            <Button icon={<ThunderboltOutlined />} onClick={() => setFillingSlots((current) => !current)}>
              Заполнить слоты
            </Button>
          )}
          <Button icon={<EditOutlined />} onClick={onEdit}>
            Редактировать
          </Button>
          {card.id != null && (
            <Button
              icon={<FileMarkdownOutlined />}
              loading={exporting}
              onClick={() => void exportMarkdown()}
            >
              В Markdown
            </Button>
          )}
          <Popconfirm
            title="Удалить карточку?"
            description="Вместе с карточкой удалятся блоки кода и разбор."
            okText="Удалить"
            cancelText="Отмена"
            okButtonProps={{ danger: true }}
            onConfirm={handleDelete}
          >
            <Button danger icon={<DeleteOutlined />} loading={deleteCard.isPending}>
              Удалить
            </Button>
          </Popconfirm>
        </Space>
      </Space>

      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        <Spin spinning={loading}>
          <Flex vertical gap={12}>
            <Space wrap size={4}>
              {card.tags.map((tag) => (
                <Tag key={tag}>{tag}</Tag>
              ))}
              {card.isTemplate && <Tag color="gold">чертёж</Tag>}
              {card.id != null && !editingTags && (
                <Button size="small" type="link" onClick={startTagEditing}>
                  Изменить теги
                </Button>
              )}
            </Space>

            {editingTags && (
              <Card size="small" title="Теги карточки">
                <Flex vertical gap={8}>
                  <TagInput
                    value={tagDraft}
                    onChange={setTagDraft}
                    known={knownTags.data ?? []}
                  />
                  <Space>
                    <Button
                      type="primary"
                      size="small"
                      loading={setCardTags.isPending}
                      onClick={saveTags}
                    >
                      Сохранить теги
                    </Button>
                    <Button size="small" onClick={() => setEditingTags(false)}>
                      Отмена
                    </Button>
                  </Space>
                </Flex>
              </Card>
            )}

            {showSlotForm && fillingSlots && (
              <SlotForm card={card} onClose={() => setFillingSlots(false)} />
            )}

            {card.description !== "" && (
              <Typography.Paragraph style={{ marginBottom: 0, whiteSpace: "pre-wrap" }}>
                {card.description}
              </Typography.Paragraph>
            )}

            {blocks.length === 0 && (
              <Typography.Text type="secondary">Блоков кода нет</Typography.Text>
            )}

            {blocks.map((block, index) => (
              <BlockCard
                key={block.id ?? index}
                block={block}
                index={index}
                copied={copiedBlock === index}
                onCopy={() => void copyBlock(index, block.code)}
              />
            ))}
          </Flex>
        </Spin>
      </div>
    </Flex>
  );
}

interface BlockCardProps {
  block: CodeBlock;
  index: number;
  copied: boolean;
  onCopy: () => void;
}

/** Блок в режиме просмотра: код слева, разбор справа, подсветка в обе стороны. */
function BlockCard({ block, index, copied, onCopy }: BlockCardProps) {
  const initialNotes = useMemo(() => toDraft(block.notes), [block.notes]);
  const notes = useNotes(initialNotes);

  return (
    <Card
      size="small"
      title={`Блок ${index + 1}`}
      extra={
        <Space>
          <Tag color="blue">{block.language}</Tag>
          <Button size="small" icon={<CopyOutlined />} onClick={onCopy}>
            {copied ? "Скопировано" : "Копировать"}
          </Button>
        </Space>
      }
    >
      <div className="card-split" style={{ maxHeight: "calc(100vh - 300px)" }}>
        <div
          style={{
            border: "1px solid rgba(255,255,255,0.12)",
            borderRadius: 6,
            minHeight: 200,
            overflow: "auto",
          }}
        >
          <CodeBlockView
            value={block.code}
            language={block.language}
            editable={false}
            ranges={notes.ranges}
            activeKey={notes.selectedKey}
            focusLine={notes.focusLine}
            focusToken={notes.focusToken}
            onLineActivate={notes.activateLine}
          />
        </div>
        <div style={{ minHeight: 200, overflow: "auto", paddingLeft: 4 }}>
          <NotesPanel notes={notes} code={block.code} editable={false} />
        </div>
      </div>
    </Card>
  );
}
