import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import {
  cardDelete,
  cardGet,
  cardSave,
  cardToggleFavorite,
  cardsList,
  search,
  type CardDetail,
  type CardSummary,
  type Filter,
  type SaveResult,
} from "../api/tauri";

// Ключи запросов собраны в одном месте: инвалидация не должна
// разбредаться по компонентам строками.
export const cardKeys = {
  all: ["cards"] as const,
  list: (filter: Filter) => ["cards", "list", filter] as const,
  detail: (id: number) => ["cards", "detail", id] as const,
};

/** Ключ списка тегов со счётчиками. */
export const tagKeys = {
  all: ["tags"] as const,
};

/** Ключи поиска: запрос и фильтр входят в ключ целиком. */
export const searchKeys = {
  all: ["search"] as const,
  query: (query: string, filter: Filter) => ["search", query, filter] as const,
};

/**
 * Поиск карточек (раздел 3.2 плана). Пустой запрос отключает выполнение:
 * в это время список показывает обычный cards_list, а не пустой результат.
 * Предыдущие результаты остаются на экране, пока идёт новый запрос,
 * чтобы карточки не мигали на каждой паузе.
 */
export function useSearch(query: string, filter: Filter = {}) {
  return useQuery<CardSummary[]>({
    queryKey: searchKeys.query(query, filter),
    queryFn: () => search(query, filter),
    enabled: query.trim() !== "",
    placeholderData: (previous) => previous,
  });
}

/** Список карточек под текущую сортировку и фильтр. */
export function useCards(filter: Filter = {}) {
  return useQuery<CardSummary[]>({
    queryKey: cardKeys.list(filter),
    queryFn: () => cardsList(filter),
  });
}

/** Одна карточка с полным содержимым. enabled=false отключает запрос для новой карточки. */
export function useCard(id: number | null) {
  return useQuery<CardDetail>({
    queryKey: cardKeys.detail(id ?? 0),
    queryFn: () => cardGet(id as number),
    enabled: id !== null,
  });
}

/**
 * Сохранение карточки. Возвращает SaveResult: при needsConfirmation
 * компонент показывает сводку и повторяет мутацию с force: true.
 * После сохранения обновляются и список, и сама карточка.
 */
export function useSaveCard(): UseMutationResult<
  SaveResult,
  Error,
  { detail: CardDetail; force?: boolean }
> {
  const queryClient = useQueryClient();
  return useMutation<SaveResult, Error, { detail: CardDetail; force?: boolean }>({
    mutationFn: ({ detail, force }) => cardSave(detail, force ?? false),
    onSuccess: (result, { detail }) => {
      // При запросе подтверждения база не менялась: инвалидировать нечего.
      if (result.status !== "ok") {
        return;
      }
      queryClient.invalidateQueries({ queryKey: cardKeys.all });
      queryClient.invalidateQueries({ queryKey: searchKeys.all });
      // Нормализация тегов в Rust может схлопнуть похожие имена: счётчики меняются.
      queryClient.invalidateQueries({ queryKey: tagKeys.all });
      if (detail.id != null) {
        queryClient.invalidateQueries({ queryKey: cardKeys.detail(detail.id) });
      }
    },
  });
}

/** Удаление карточки с обновлением списка. */
export function useDeleteCard(): UseMutationResult<void, Error, number> {
  const queryClient = useQueryClient();
  return useMutation<void, Error, number>({
    mutationFn: (id) => cardDelete(id),
    onSuccess: (_result, id) => {
      queryClient.removeQueries({ queryKey: cardKeys.detail(id) });
      queryClient.invalidateQueries({ queryKey: cardKeys.all });
      queryClient.invalidateQueries({ queryKey: searchKeys.all });
      queryClient.invalidateQueries({ queryKey: tagKeys.all });
    },
  });
}

/**
 * Переключение избранного. Команда возвращает новое значение избранного,
 * поэтому карточка правится прямо в кэше, без повторного чтения.
 * Пока мутация выполняется, кнопка блокируется, чтобы не было двойных щелчков.
 */
export function useToggleFavorite(): UseMutationResult<boolean, Error, number> {
  const queryClient = useQueryClient();
  return useMutation<boolean, Error, number>({
    mutationFn: (id) => cardToggleFavorite(id),
    onSuccess: (favorite, id) => {
      // Новое значение пришло от Rust: карточку правим в кэше без лишнего запроса.
      queryClient.setQueryData<CardDetail>(cardKeys.detail(id), (current) =>
        current ? { ...current, favorite } : current,
      );
      // Списки отфильтрованы по избранному на стороне Rust: их нужно перечитать.
      queryClient.invalidateQueries({ queryKey: cardKeys.all });
    },
  });
}
