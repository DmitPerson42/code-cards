import { App, Button, Dropdown } from "antd";
import { FolderOutlined } from "@ant-design/icons";
import { errorText } from "../api/tauri";
import { flattenCollections, useAddCardToCollection, useCollections } from "../hooks/useCollections";

// Кнопка "В подборку" (раздел 3.3 плана, этап 5):
// меню со всеми подборками, добавление карточки в конец выбранной.
// Одна и та же кнопка используется и в карточке, и в плитке списка.

export interface AddToCollectionProps {
  cardId: number;
  /** true - компактная кнопка только с иконкой (для плитки списка). */
  compact?: boolean;
  /** Не всплывать до клика по плитке списка. */
  stopPropagation?: boolean;
}

export default function AddToCollection({
  cardId,
  compact = false,
  stopPropagation = false,
}: AddToCollectionProps) {
  const { message } = App.useApp();
  const collections = useCollections();
  const addCard = useAddCardToCollection();

  // Дерево приходит с отступами: в меню плоский список с вложенностью в подписи.
  const items = flattenCollections(collections.data ?? []).map(({ node, depth }) => ({
    key: String(node.id),
    label: `${"\u00a0\u00a0".repeat(depth)}${node.name}`,
  }));

  if (collections.isSuccess && items.length === 0) {
    return null;
  }

  return (
    <Dropdown
      disabled={collections.isPending}
      menu={{
        items:
          items.length > 0
            ? items
            : [{ key: "none", label: "Подборок пока нет", disabled: true }],
        onClick: ({ key, domEvent }) => {
          if (stopPropagation) {
            domEvent.stopPropagation();
          }
          const collectionId = Number(key);
          addCard.mutate(
            { collectionId, cardId },
            {
              onSuccess: () => message.success("Карточка добавлена в подборку"),
              onError: (error) =>
                message.error(`Не удалось добавить в подборку: ${errorText(error)}`),
            },
          );
        },
      }}
    >
      <Button
        size={compact ? "small" : "middle"}
        icon={<FolderOutlined />}
        loading={addCard.isPending}
        onClick={(event) => {
          if (stopPropagation) {
            event.stopPropagation();
          }
        }}
      >
        {compact ? undefined : "В подборку"}
      </Button>
    </Dropdown>
  );
}
