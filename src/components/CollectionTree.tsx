import { useMemo, useState } from "react";
import { App, Button, Empty, Flex, Input, Space, Tooltip, Tree, Typography } from "antd";
import type { DataNode } from "antd/es/tree";
import {
  DeleteOutlined,
  EditOutlined,
  FolderAddOutlined,
  InboxOutlined,
  PlusOutlined,
} from "@ant-design/icons";
import { errorText, type CollectionNode } from "../api/tauri";
import {
  useDeleteCollection,
  useSaveCollection,
  useSetCollectionCards,
} from "../hooks/useCollections";

// Дерево подборок (раздел 3.4 плана, этап 5): создание, переименование,
// удаление с подтверждением, переход по подборке фильтрует список карточек.
// Карточку можно перетащить прямо на узел дерева (HTML5 drag and drop).

/** Тип перетаскиваемого объекта: карточка из списка. */
export const CARD_DRAG_TYPE = "application/x-code-cards-card";

export interface CollectionTreeProps {
  /** Дерево от Rust, уже упорядоченное. */
  nodes: CollectionNode[];
  loading: boolean;
  /** Выбранная подборка: null - все карточки. */
  selectedId: number | null;
  onSelect: (id: number | null) => void;
}

export default function CollectionTree({
  nodes,
  loading,
  selectedId,
  onSelect,
}: CollectionTreeProps) {
  const { message, modal } = App.useApp();
  const saveCollection = useSaveCollection();
  const deleteCollection = useDeleteCollection();
  const setCollectionCards = useSetCollectionCards();

  // Инлайн-правка имени: либо новая подборка в parentId, либо переименование id.
  const [editing, setEditing] = useState<{
    id: number | null;
    parentId: number | null;
    name: string;
  } | null>(null);
  const [dragOverId, setDragOverId] = useState<number | null>(null);

  /** id всех карточек подборки в текущем порядке: порядок задаёт cards_list. */
  async function readCardIds(collectionId: number): Promise<number[]> {
    const { cardsList } = await import("../api/tauri");
    const cards = await cardsList({ collectionId });
    return cards.map((card) => card.id);
  }

  async function handleDropOnCollection(event: React.DragEvent, collectionId: number) {
    event.preventDefault();
    event.stopPropagation();
    setDragOverId(null);
    const raw = event.dataTransfer.getData(CARD_DRAG_TYPE);
    if (raw === "") {
      return;
    }
    const cardId = Number(raw);
    if (!Number.isFinite(cardId)) {
      return;
    }
    try {
      const current = await readCardIds(collectionId);
      if (current.includes(cardId)) {
        return;
      }
      // collection_set_cards перезаписывает состав целиком, порядок 0..n.
      await setCollectionCards.mutateAsync({
        collectionId,
        cardIds: [...current, cardId],
      });
      message.success("Карточка добавлена в подборку");
    } catch (error) {
      message.error(
        `Не удалось добавить карточку: ${errorText(error)}`,
      );
    }
  }

  function startCreate(parentId: number | null) {
    setEditing({ id: null, parentId, name: "" });
  }

  function startRename(node: CollectionNode) {
    setEditing({ id: node.id, parentId: node.parentId, name: node.name });
  }

  function commitEditing() {
    if (editing === null) {
      return;
    }
    const name = editing.name.trim();
    if (name === "") {
      message.warning("Имя подборки не может быть пустым");
      return;
    }
    saveCollection.mutate(
      { id: editing.id, name, parentId: editing.parentId },
      {
        onSuccess: (id) => {
          setEditing(null);
          message.success(editing.id === null ? "Подборка создана" : "Имя изменено");
          if (editing.id === null) {
            onSelect(id);
          }
        },
        // Rust отклоняет пустое имя и цикл: показываем его текст как есть.
        onError: (error) => message.error(errorText(error)),
      },
    );
  }

  function confirmDelete(node: CollectionNode) {
    modal.confirm({
      title: `Удалить подборку «${node.name}»?`,
      content: "Вложенные подборки тоже удалятся. Карточки останутся в библиотеке.",
      okText: "Удалить",
      cancelText: "Отмена",
      okButtonProps: { danger: true },
      onOk: () => {
        deleteCollection.mutate(node.id, {
          onSuccess: () => {
            message.success("Подборка удалена");
            if (selectedId === node.id) {
              onSelect(null);
            }
          },
          onError: (error) => message.error(`Не удалось удалить: ${errorText(error)}`),
        });
      },
    });
  }

  const treeData = useMemo<DataNode[]>(() => build(nodes), [nodes]);

  function build(list: CollectionNode[]): DataNode[] {
    return list.map((node) => ({
      key: String(node.id),
      title: (
        <Flex
          align="center"
          justify="space-between"
          gap={6}
          // Узел принимает карточку: это и есть перетаскивание в подборку.
          onDragOver={(event) => {
            if (event.dataTransfer.types.includes(CARD_DRAG_TYPE)) {
              event.preventDefault();
              event.stopPropagation();
              setDragOverId(node.id);
            }
          }}
          onDragLeave={() => setDragOverId((current) => (current === node.id ? null : current))}
          onDrop={(event) => void handleDropOnCollection(event, node.id)}
          style={{
            borderRadius: 4,
            outline: dragOverId === node.id ? "1px dashed #5aaaff" : "none",
            outlineOffset: 2,
          }}
        >
          <Space size={4}>
            <span>{node.name}</span>
            <Typography.Text type="secondary" className="count-chip">
              {node.cardCount}
            </Typography.Text>
          </Space>
          <Space size={0}>
            <Tooltip title="Добавить вложенную подборку">
              <Button
                type="text"
                size="small"
                aria-label="Добавить вложенную подборку"
                icon={<FolderAddOutlined />}
                onClick={(event) => {
                  event.stopPropagation();
                  startCreate(node.id);
                }}
              />
            </Tooltip>
            <Tooltip title="Переименовать">
              <Button
                type="text"
                size="small"
                aria-label="Переименовать подборку"
                icon={<EditOutlined />}
                onClick={(event) => {
                  event.stopPropagation();
                  startRename(node);
                }}
              />
            </Tooltip>
            <Tooltip title="Удалить подборку">
              <Button
                type="text"
                size="small"
                danger
                aria-label="Удалить подборку"
                icon={<DeleteOutlined />}
                onClick={(event) => {
                  event.stopPropagation();
                  confirmDelete(node);
                }}
              />
            </Tooltip>
          </Space>
        </Flex>
      ),
      children: build(node.children),
    }));
  }

  return (
    <Flex vertical gap={6}>
      <Space style={{ justifyContent: "space-between", width: "100%" }}>
        <Typography.Text strong>Подборки</Typography.Text>
        <Tooltip title="Новая подборка">
          <Button
            type="text"
            size="small"
            aria-label="Новая подборка"
            icon={<PlusOutlined />}
            onClick={() => startCreate(null)}
          />
        </Tooltip>
      </Space>

      {editing !== null && (
        <Input
          size="small"
          autoFocus
          value={editing.name}
          placeholder="Имя подборки"
          onChange={(event) => setEditing({ ...editing, name: event.target.value })}
          onPressEnter={commitEditing}
          onBlur={() => {
            // Пустая строка не сохраняется и не оставляет висящий ввод.
            if (editing.name.trim() === "") {
              setEditing(null);
              return;
            }
            commitEditing();
          }}
        />
      )}

      <Button
        type={selectedId === null ? "primary" : "text"}
        size="small"
        block
        style={{ textAlign: "left" }}
        icon={<InboxOutlined />}
        onClick={() => onSelect(null)}
      >
        Все карточки
      </Button>

      {loading && <Typography.Text type="secondary">Загрузка...</Typography.Text>}

      {!loading && treeData.length === 0 && (
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description="Подборок нет"
          style={{ margin: 8 }}
        />
      )}

      {!loading && treeData.length > 0 && (
        <Tree
          treeData={treeData}
          blockNode
          // Перетаскивание внутри дерева не нужно: порядок подборок задаёт Rust.
          draggable={false}
          selectedKeys={selectedId === null ? [] : [String(selectedId)]}
          onSelect={(keys) => {
            const key = keys[0];
            onSelect(key == null ? null : Number(key));
          }}
        />
      )}

      <Typography.Text type="secondary" style={{ fontSize: 12 }}>
        Подсказка: карточку можно перетащить на подборку
      </Typography.Text>
    </Flex>
  );
}
