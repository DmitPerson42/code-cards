import { useEffect, useRef, useState } from "react";
import {
  App,
  Button,
  Card,
  Empty,
  Flex,
  Grid,
  Input,
  Segmented,
  Select,
  Skeleton,
  Space,
  Tag,
  Tooltip,
  Typography,
  type InputRef,
} from "antd";
import { LoadingOutlined, PlusOutlined, StarFilled, StarOutlined } from "@ant-design/icons";
import { errorText, type CardSort, type CardSummary, type Filter } from "../api/tauri";
import { useCards, useSearch, useToggleFavorite } from "../hooks/useCards";
import { useCollections, useSetCollectionCards } from "../hooks/useCollections";
import { useDebouncedValue } from "../hooks/useDebouncedValue";
import { useTags } from "../hooks/useTags";
import { MONO_FONT } from "../theme/antdTheme";
import { HighlightedText } from "../utils/highlight";
import AddToCollection from "./AddToCollection";
import CollectionTree, { CARD_DRAG_TYPE } from "./CollectionTree";

// Экран списка карточек (раздел 3.4 плана): живой поиск с подсветкой,
// панель тегов со счётчиками, фильтры и сортировка.
// Панель подборок подключается на этапе 5.

const SORT_OPTIONS: ReadonlyArray<{ value: CardSort; label: string }> = [
  { value: "recent", label: "Недавние" },
  { value: "alpha", label: "По алфавиту" },
  { value: "created", label: "По дате создания" },
];

const LANGUAGE_OPTIONS = [
  { value: "", label: "Все языки" },
  { value: "python", label: "Python" },
  { value: "javascript", label: "JavaScript" },
  { value: "typescript", label: "TypeScript" },
  { value: "sql", label: "SQL" },
  { value: "html", label: "HTML" },
  { value: "css", label: "CSS" },
  { value: "markdown", label: "Markdown" },
  { value: "json", label: "JSON" },
  { value: "shell", label: "Shell" },
  { value: "text", label: "Текст" },
];

export interface CardListProps {
  onOpen: (id: number) => void;
  onCreate: () => void;
}

/** Первые строки превью: в плитке помещается 3-4 строки кода. */
function previewLines(code: string, limit = 4): string {
  const lines = code.replace(/\s+$/, "").split("\n").slice(0, limit);
  return lines.join("\n");
}

export default function CardList({ onOpen, onCreate }: CardListProps) {
  const { message } = App.useApp();
  const screens = Grid.useBreakpoint();
  const [sort, setSort] = useState<CardSort>("recent");
  const [language, setLanguage] = useState<string>("");
  const [favoriteOnly, setFavoriteOnly] = useState(false);
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  // Выбранная подборка: null - все карточки библиотеки.
  const [collectionId, setCollectionId] = useState<number | null>(null);
  // В запрос уходит только значение после паузы, поле остаётся отзывчивым.
  const debouncedQuery = useDebouncedValue(query.trim(), 200);
  const searching = debouncedQuery !== "";
  const searchInputRef = useRef<InputRef | null>(null);

  const filter: Filter = {
    sort,
    language: language === "" ? null : language,
    favoriteOnly,
    // Теги комбинируются по И: Rust требует присутствия всех указанных.
    tags: selectedTags,
    // Выбранная подборка задаёт и состав, и порядок карточек внутри неё.
    collectionId,
  };
  const cards = useCards(filter);
  const searchQuery = useSearch(debouncedQuery, filter);
  // Один источник данных для разметки: поиск, когда есть запрос, иначе список.
  const active = searching ? searchQuery : cards;
  const tags = useTags();
  const collections = useCollections();
  const toggleFavorite = useToggleFavorite();
  const setCollectionCards = useSetCollectionCards();
  // Карточка, которую сейчас переставляют внутри подборки.
  const [draggingId, setDraggingId] = useState<number | null>(null);

  /**
   * Перестановка карточки внутри подборки. Порядок хранится целиком списком,
   * поэтому собранный массив сразу уходит в collection_set_cards.
   */
  function moveCard(cardId: number, targetIndex: number) {
    if (collectionId === null || !cards.isSuccess) {
      return;
    }
    const ids = cards.data.map((card) => card.id);
    const from = ids.indexOf(cardId);
    if (from === -1) {
      return;
    }
    const without = ids.filter((id) => id !== cardId);
    const insertAt = Math.max(0, Math.min(targetIndex > from ? targetIndex - 1 : targetIndex, without.length));
    without.splice(insertAt, 0, cardId);
    if (without.join(",") === ids.join(",")) {
      return;
    }
    setCollectionCards.mutate(
      { collectionId, cardIds: without },
      {
        onError: (error) => message.error(`Не удалось изменить порядок: ${errorText(error)}`),
      },
    );
  }

  // Ctrl+F переводит курсор в поле поиска (раздел 3.7 плана).
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        searchInputRef.current?.focus();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  function toggleTagFilter(name: string) {
    setSelectedTags((current) =>
      current.includes(name)
        ? current.filter((tag) => tag !== name)
        : [...current, name],
    );
  }

  function handleFavorite(card: CardSummary, event: React.MouseEvent) {
    event.stopPropagation();
    toggleFavorite.mutate(card.id, {
      onError: (error) => {
        message.error(`Не удалось изменить избранное: ${errorText(error)}`);
      },
    });
  }

  return (
    <Flex gap={16} style={{ height: "100%", minHeight: 0, alignItems: "stretch" }}>
      {/* Левая панель: подборки (дерево) и теги со счётчиками (раздел 3.4 плана) */}
      <Flex vertical gap={12} style={{ width: 240, flex: "0 0 240px", minHeight: 0, overflowY: "auto" }}>
        <Card size="small" bodyStyle={{ padding: 8 }}>
          <CollectionTree
            nodes={collections.data ?? []}
            loading={collections.isPending}
            selectedId={collectionId}
            onSelect={setCollectionId}
          />
        </Card>
      <Card size="small" title="Теги" style={{ overflowY: "auto" }}>
        {tags.isPending && <Skeleton active paragraph={{ rows: 4 }} />}
        {tags.isError && (
          <Typography.Text type="danger">
            Не удалось загрузить теги: {errorText(tags.error)}
          </Typography.Text>
        )}
        {tags.isSuccess && tags.data.length === 0 && (
          <Typography.Text type="secondary">Тегов пока нет</Typography.Text>
        )}
        {tags.isSuccess && tags.data.length > 0 && (
          <Flex vertical gap={2}>
            {tags.data.map((tag) => {
              const active = selectedTags.includes(tag.name);
              return (
                <Button
                  key={tag.id}
                  type={active ? "primary" : "text"}
                  size="small"
                  block
                  style={{ textAlign: "left", justifyContent: "space-between" }}
                  onClick={() => toggleTagFilter(tag.name)}
                >
                  <span>{tag.name}</span>
                  <span className="count-chip">{tag.count}</span>
                </Button>
              );
            })}
            {selectedTags.length > 0 && (
              <Button size="small" type="link" onClick={() => setSelectedTags([])}>
                Сбросить теги
              </Button>
            )}
          </Flex>
        )}
      </Card>
      </Flex>

      <Flex vertical gap={16} style={{ flex: 1, minWidth: 0, minHeight: 0 }}>
        {/* Строка управления списком */}
        <Space wrap size="middle" style={{ justifyContent: "space-between", width: "100%" }}>
          <Space wrap size="middle">
            <Input
              ref={searchInputRef}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Поиск по названию, коду, тегам и разбору (Ctrl+F)"
              allowClear
              style={{ width: 320 }}
              suffix={
                query !== "" && searchQuery.isFetching ? <LoadingOutlined /> : undefined
              }
            />
            <Select
              value={language}
              onChange={setLanguage}
              options={LANGUAGE_OPTIONS}
              style={{ width: 160 }}
            />
            <Segmented
              value={sort}
              onChange={(value) => setSort(value as CardSort)}
              options={SORT_OPTIONS.map((option) => ({
                value: option.value,
                label: option.label,
              }))}
            />
            <Tooltip title={favoriteOnly ? "Показать все карточки" : "Только избранные"}>
              <Button
                type={favoriteOnly ? "primary" : "default"}
                icon={favoriteOnly ? <StarFilled /> : <StarOutlined />}
                onClick={() => setFavoriteOnly((current) => !current)}
              >
                Избранное
              </Button>
            </Tooltip>
            {selectedTags.map((tag) => (
              <Tag key={tag} closable onClose={() => toggleTagFilter(tag)} color="blue">
                {tag}
              </Tag>
            ))}
          </Space>
          <Button type="primary" icon={<PlusOutlined />} onClick={onCreate}>
            Новая карточка
          </Button>
        </Space>

      {/* Область сетки: единственный скроллящийся блок экрана */}
      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        {active.isPending && (
          <Space wrap size="middle">
            {[0, 1, 2, 3].map((key) => (
              <Card key={key} style={{ width: 320 }} className="card-tile">
                <Skeleton active paragraph={{ rows: 3 }} />
              </Card>
            ))}
          </Space>
        )}

        {active.isError && (
          <Empty description={`Не удалось загрузить карточки: ${errorText(active.error)}`}>
            <Button onClick={() => void active.refetch()}>Повторить</Button>
          </Empty>
        )}

        {active.isSuccess && active.data.length === 0 && searching && (
          <Empty
            description={
              <Space direction="vertical" size={4}>
                <Typography.Text strong>Ничего не найдено</Typography.Text>
                <Typography.Text type="secondary">
                  Поиск точный: опечатки не исправляются, слова находятся по началу,
                  а символы вроде arr[0] ищутся без пробелов внутри.
                </Typography.Text>
              </Space>
            }
          >
            <Button onClick={() => setQuery("")}>Очистить поиск</Button>
          </Empty>
        )}

        {active.isSuccess && active.data.length === 0 && !searching && collectionId !== null && (
          <Empty description="В этой подборке пока нет карточек">
            <Space direction="vertical" size={8}>
              <Typography.Text type="secondary">
                Перетащите карточку на подборку в левой панели или нажмите «В подборку» на карточке.
              </Typography.Text>
              <Button onClick={() => setCollectionId(null)}>Все карточки</Button>
            </Space>
          </Empty>
        )}

        {active.isSuccess && active.data.length === 0 && !searching && collectionId === null && (
          <Empty description="Карточек нет">
            <Button icon={<PlusOutlined />} onClick={onCreate}>
              Новая карточка
            </Button>
          </Empty>
        )}

        {active.isSuccess && active.data.length > 0 && (
          <Flex wrap gap={16} align="stretch">
            {active.data.map((card, index) => (
              <Card
                key={card.id}
                className="card-tile"
                style={{
                  width: screens.xl ? 340 : 300,
                  // Внутри подборки карточки переставляются перетаскиванием.
                  cursor: collectionId === null ? "pointer" : "grab",
                  opacity: draggingId === card.id ? 0.5 : 1,
                }}
                hoverable
                onClick={() => onOpen(card.id)}
                draggable
                onDragStart={(event) => {
                  // Карточку можно перетащить на подборку в дереве и по списку.
                  event.dataTransfer.setData(CARD_DRAG_TYPE, String(card.id));
                  event.dataTransfer.effectAllowed = "move";
                  setDraggingId(card.id);
                }}
                onDragEnd={() => setDraggingId(null)}
                onDragOver={(event) => {
                  if (collectionId !== null && draggingId !== null) {
                    event.preventDefault();
                  }
                }}
                onDrop={(event) => {
                  if (collectionId === null || draggingId === null) {
                    return;
                  }
                  event.preventDefault();
                  event.stopPropagation();
                  moveCard(draggingId, index);
                  setDraggingId(null);
                }}
                title={
                  <Flex justify="space-between" align="center" gap={8}>
                    <Typography.Text ellipsis style={{ maxWidth: 200 }} title={card.title}>
                      {searching ? (
                        <HighlightedText text={card.title} query={debouncedQuery} />
                      ) : (
                        card.title
                      )}
                    </Typography.Text>
                    <Space size={0}>
                      <Tooltip title={card.favorite ? "Убрать из избранного" : "В избранное"}>
                        <Button
                          type="text"
                          size="small"
                          aria-label={card.favorite ? "Убрать из избранного" : "Добавить в избранное"}
                          icon={card.favorite ? <StarFilled style={{ color: "#f0b429" }} /> : <StarOutlined />}
                          loading={toggleFavorite.isPending && toggleFavorite.variables === card.id}
                          onClick={(event) => handleFavorite(card, event)}
                        />
                      </Tooltip>
                      <AddToCollection cardId={card.id} compact stopPropagation />
                    </Space>
                  </Flex>
                }
              >
                <Flex vertical gap={8} style={{ height: "100%" }}>
                  <Space wrap size={4}>
                    <Tag color="blue">{card.language}</Tag>
                    {card.isTemplate && <Tag color="gold">чертёж</Tag>}
                    {card.tags.map((tag) => (
                      <Tag key={tag}>
                        {searching ? (
                          <HighlightedText text={tag} query={debouncedQuery} />
                        ) : (
                          tag
                        )}
                      </Tag>
                    ))}
                  </Space>

                  {card.description !== "" && (
                    <Typography.Paragraph
                      type="secondary"
                      ellipsis={{ rows: 2 }}
                      style={{ marginBottom: 0 }}
                    >
                      {searching ? (
                        <HighlightedText text={card.description} query={debouncedQuery} />
                      ) : (
                        card.description
                      )}
                    </Typography.Paragraph>
                  )}

                  {/* Сниппет поиска важнее превью: в нём видно, где совпадение */}
                  {searching && card.snippet != null && card.snippet !== "" && (
                    <pre
                      className="code-surface search-snippet"
                      style={{ fontFamily: MONO_FONT, fontSize: 12, flex: 1 }}
                    >
                      <HighlightedText text={card.snippet} query={debouncedQuery} />
                    </pre>
                  )}

                  {(!searching || card.snippet == null || card.snippet === "") &&
                    card.preview !== "" && (
                      <pre
                        className="code-surface"
                        style={{ fontFamily: MONO_FONT, fontSize: 12, flex: 1 }}
                      >
                        {previewLines(card.preview)}
                      </pre>
                    )}
                </Flex>
              </Card>
            ))}
          </Flex>
        )}
      </div>
      </Flex>
    </Flex>
  );
}
