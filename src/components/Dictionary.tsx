import { useEffect, useRef, useState } from "react";
import {
  Alert,
  App,
  Button,
  Card,
  Empty,
  Flex,
  Input,
  Modal,
  Popconfirm,
  Skeleton,
  Space,
  Tag,
  Tooltip,
  Typography,
  type InputRef,
} from "antd";
import {
  CopyOutlined,
  DeleteOutlined,
  EditOutlined,
  FileTextOutlined,
  LoadingOutlined,
  PlusOutlined,
  SearchOutlined,
} from "@ant-design/icons";
import { errorText } from "../api/tauri";
import { useDebouncedValue } from "../hooks/useDebouncedValue";
import {
  useDeleteTerm,
  useSaveTerm,
  useTermSearch,
  useTerms,
  type Term,
  type TermHit,
} from "../hooks/useTerms";
import { copyText } from "../utils/clipboard";
import { HighlightedText } from "../utils/highlight";

// Вкладка "Словарь": термины и их значения.
//
// Устройство простое, как у заметок: сверху поиск и кнопка добавления,
// ниже либо весь словарь по алфавиту, либо результаты поиска.
// Значение - свободный текст, поэтому под каждым термином его видно
// целиком, без раскрытия по клику: словарь читают, а не разгадывают.

export interface DictionaryProps {
  /** Запрос пришёл из общего поля поиска на вкладке "Карточки". */
  initialQuery?: string;
}

/** Значение в одну строку: для превью в списке и для копирования. */
function oneLine(text: string, limit = 300): string {
  const flat = text.split("\n").map((line) => line.trim()).filter(Boolean).join(" · ");
  return flat.length > limit ? `${flat.slice(0, limit)}...` : flat;
}

export default function Dictionary({ initialQuery = "" }: DictionaryProps) {
  const { message } = App.useApp();
  const [filter, setFilter] = useState("");
  const [editing, setEditing] = useState<Term | null>(null);
  const [creating, setCreating] = useState(false);
  const searchInputRef = useRef<InputRef | null>(null);
  const debouncedQuery = useDebouncedValue(filter.trim(), 200);

  // Запрос из вкладки карточек подставляется один раз, при открытии.
  useEffect(() => {
    if (initialQuery.trim() !== "") {
      setFilter(initialQuery);
    }
  }, [initialQuery]);

  const list = useTerms(debouncedQuery);
  const search = useTermSearch(debouncedQuery);
  const searching = debouncedQuery !== "";
  // Один источник строк для разметки: результаты поиска, пока есть запрос,
  // иначе весь словарь. Поиск смотрит по словам и по значению, список
  // фильтрует по подстроке: вместе это покрывает и "асинхр", и "ASGI серв".
  const active = searching ? search : list;

  // Ctrl+F переводит курсор в поле поиска вкладки.
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

  function startCreate() {
    setEditing(null);
    setCreating(true);
  }

  function startEdit(term: Term) {
    setCreating(false);
    setEditing(term);
  }

  return (
    <Flex vertical gap={16} style={{ height: "100%", minHeight: 0 }}>
      <Space wrap size="middle" style={{ justifyContent: "space-between", width: "100%" }}>
        <Space wrap size="middle">
          <Input
            ref={searchInputRef}
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder="Поиск по словарю: термин или значение (Ctrl+F)"
            allowClear
            prefix={<SearchOutlined />}
            style={{ width: 380 }}
            suffix={filter !== "" && search.isFetching ? <LoadingOutlined /> : undefined}
          />
          <Tag>{`терминов: ${list.data?.length ?? 0}`}</Tag>
        </Space>
        <Button type="primary" icon={<PlusOutlined />} onClick={startCreate}>
          Новый термин
        </Button>
      </Space>

      <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
        {active.isPending && <Skeleton active paragraph={{ rows: 6 }} />}

        {active.isError && (
          <Empty
            description={`Не удалось прочитать словарь: ${errorText(active.error)}`}
          >
            <Button onClick={() => void active.refetch()}>Повторить</Button>
          </Empty>
        )}

        {active.isSuccess && searching && (
          <SearchResults
            query={debouncedQuery}
            hits={search.data ?? []}
            listTerms={list.data ?? []}
            onEdit={startEdit}
            onClear={() => setFilter("")}
          />
        )}

        {active.isSuccess && !searching && (list.data ?? []).length === 0 && (
          <Empty description="Словарь пуст">
            <Space direction="vertical" size={8}>
              <Typography.Text type="secondary">
                Записывайте сюда термины и их значения: их найдёт общий поиск.
              </Typography.Text>
              <Button icon={<PlusOutlined />} onClick={startCreate}>
                Добавить первый термин
              </Button>
            </Space>
          </Empty>
        )}

        {active.isSuccess && !searching && (list.data ?? []).length > 0 && (
          <Flex vertical gap={8}>
            {(list.data ?? []).map((term) => (
              <TermRow
                key={term.id ?? term.term}
                term={term}
                query=""
                onEdit={() => startEdit(term)}
              />
            ))}
          </Flex>
        )}
      </div>

      <TermForm
        open={creating || editing !== null}
        term={editing}
        onClose={() => {
          setCreating(false);
          setEditing(null);
        }}
        onSaved={(text) => message.success(text)}
      />
    </Flex>
  );
}

/** Один термин в списке: название, значение, действия. */
function TermRow({
  term,
  query,
  onEdit,
}: {
  term: Term;
  query: string;
  onEdit: () => void;
}) {
  const { message } = App.useApp();
  const remove = useDeleteTerm();

  async function copyMeaning() {
    try {
      await copyText(`${term.term}: ${term.meaning}`);
      message.success("Термин скопирован");
    } catch (error) {
      message.error(errorText(error));
    }
  }

  return (
    <Card size="small" bodyStyle={{ padding: 10 }}>
      <Flex vertical gap={6}>
        <Flex justify="space-between" align="center" gap={8} wrap>
          <Space size={8} align="baseline" wrap>
            <Typography.Text strong>
              <HighlightedText text={term.term} query={query} />
            </Typography.Text>
          </Space>
          <Space size={0}>
            <Tooltip title="Скопировать термин и значение">
              <Button
                size="small"
                type="text"
                icon={<CopyOutlined />}
                aria-label="Скопировать термин"
                onClick={() => void copyMeaning()}
              />
            </Tooltip>
            <Tooltip title="Изменить">
              <Button
                size="small"
                type="text"
                icon={<EditOutlined />}
                aria-label="Изменить термин"
                onClick={onEdit}
              />
            </Tooltip>
            <Popconfirm
              title="Удалить термин?"
              description="Значение тоже пропадёт из словаря."
              okText="Удалить"
              cancelText="Отмена"
              okButtonProps={{ danger: true }}
              onConfirm={() =>
                remove.mutate(term.id as number, {
                  onSuccess: () => message.success("Термин удалён"),
                  onError: (error) =>
                    message.error(`Не удалось удалить: ${errorText(error)}`),
                })
              }
            >
              <Button
                size="small"
                type="text"
                danger
                icon={<DeleteOutlined />}
                aria-label="Удалить термин"
                loading={remove.isPending}
              />
            </Popconfirm>
          </Space>
        </Flex>
        {term.meaning.trim() === "" ? (
          <Typography.Text type="secondary">Значения пока нет</Typography.Text>
        ) : (
          <Typography.Paragraph
            style={{ marginBottom: 0, whiteSpace: "pre-wrap" }}
            ellipsis={{ rows: 4, expandable: true, symbol: "показать целиком" }}
          >
            <HighlightedText text={term.meaning} query={query} />
          </Typography.Paragraph>
        )}
      </Flex>
    </Card>
  );
}

/** Результаты поиска: сначала совпадения по термину, потом по значению. */
function SearchResults({
  query,
  hits,
  listTerms,
  onEdit,
  onClear,
}: {
  query: string;
  hits: TermHit[];
  /** Термины, у которых введённый текст есть внутри слова (подстрока). */
  listTerms: Term[];
  onEdit: (term: Term) => void;
  onClear: () => void;
}) {
  // Поиск ищет слова по началу, а список фильтрует по подстроке.
  // Объединение даёт то, чего ждёт пользователь: "синхрон" найдёт
  // "асинхронность", хотя по словам такой префикс не совпадает.
  const byTerm: TermHit[] = [];
  const seen = new Set<number>();
  for (const hit of hits) {
    if (!hit.crumb && !seen.has(hit.id)) {
      seen.add(hit.id);
      byTerm.push(hit);
    }
  }
  for (const term of listTerms) {
    const id = term.id as number;
    if (!seen.has(id)) {
      seen.add(id);
      byTerm.push({
        id,
        term: term.term,
        meaning: term.meaning,
        snippet: oneLine(term.meaning),
        crumb: false,
      });
    }
  }
  const byMeaning = hits.filter((hit) => hit.crumb && !seen.has(hit.id));

  if (byTerm.length === 0 && byMeaning.length === 0) {
    return (
      <Empty
        description={
          <Space direction="vertical" size={4}>
            <Typography.Text strong>В словаре ничего не найдено</Typography.Text>
            <Typography.Text type="secondary">
              Поиск идёт по термину и по значению. Слова находятся по началу,
              символы вроде arr[0] - как есть.
            </Typography.Text>
          </Space>
        }
      >
        <Button onClick={onClear}>Очистить поиск</Button>
      </Empty>
    );
  }

  return (
    <Flex vertical gap={8}>
      {byTerm.map((hit) => (
        <Card key={hit.id} size="small" bodyStyle={{ padding: 10 }}>
          <Flex vertical gap={4}>
            <Typography.Text strong>
              <HighlightedText text={hit.term} query={query} />
            </Typography.Text>
            {hit.snippet !== "" && (
              <pre className="code-surface search-snippet" style={{ fontSize: 12 }}>
                <HighlightedText text={hit.snippet} query={query} />
              </pre>
            )}
          </Flex>
        </Card>
      ))}

      {byMeaning.length > 0 && (
        <Card size="small" title="Встречается в значениях" bodyStyle={{ padding: 10 }}>
          <Flex vertical gap={6}>
            {byMeaning.map((hit) => (
              <Flex key={hit.id} vertical gap={2}>
                <Space size={6} align="baseline">
                  <Typography.Text strong>{hit.term}</Typography.Text>
                  <Tooltip title="Открыть термин">
                    <Button
                      size="small"
                      type="link"
                      icon={<FileTextOutlined />}
                      onClick={() => onEdit({ ...hit, createdAt: "", updatedAt: "" })}
                    />
                  </Tooltip>
                </Space>
                <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                  <HighlightedText text={hit.snippet} query={query} />
                </Typography.Text>
              </Flex>
            ))}
          </Flex>
        </Card>
      )}
    </Flex>
  );
}

/** Форма термина: название и значение. Одна и та же для создания и правки. */
function TermForm({
  open,
  term,
  onClose,
  onSaved,
}: {
  open: boolean;
  term: Term | null;
  onClose: () => void;
  onSaved: (message: string) => void;
}) {
  const [name, setName] = useState("");
  const [meaning, setMeaning] = useState("");
  const save = useSaveTerm();

  // Поля заполняются при каждом открытии формы: пустые для нового термина.
  useEffect(() => {
    if (open) {
      setName(term?.term ?? "");
      setMeaning(term?.meaning ?? "");
    }
  }, [open, term]);

  function submit() {
    if (name.trim() === "") {
      return;
    }
    save.mutate(
      { id: term?.id ?? null, term: name, meaning },
      {
        onSuccess: () => {
          onSaved(term === null ? "Термин добавлен" : "Термин обновлён");
          onClose();
        },
      },
    );
  }

  return (
    <Modal
      open={open}
      title={term === null ? "Новый термин" : "Термин"}
      onCancel={onClose}
      width={560}
      footer={
        <Space>
          <Button onClick={onClose}>Отмена</Button>
          <Button
            type="primary"
            loading={save.isPending}
            disabled={name.trim() === ""}
            onClick={submit}
          >
            Сохранить
          </Button>
        </Space>
      }
    >
      <Flex vertical gap={12}>
        <div>
          <Typography.Text strong>Термин</Typography.Text>
          <Input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Например: асинхронность"
            autoFocus
            onPressEnter={submit}
            maxLength={200}
          />
        </div>
        <div>
          <Typography.Text strong>Значение</Typography.Text>
          <Input.TextArea
            value={meaning}
            onChange={(event) => setMeaning(event.target.value)}
            placeholder="Что это значит, как применять, примеры"
            autoSize={{ minRows: 4, maxRows: 12 }}
          />
        </div>
        {save.isError && (
          <Alert
            type="error"
            showIcon
            message="Не удалось сохранить"
            description={errorText(save.error)}
          />
        )}
        <Typography.Text type="secondary">
          В словаре на один термин приходится одно значение. Повторяющийся
          термин сохранить нельзя: откройте существующий и поправьте его.
        </Typography.Text>
      </Flex>
    </Modal>
  );
}
