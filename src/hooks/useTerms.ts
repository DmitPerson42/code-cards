import { useMutation, useQuery, useQueryClient, type UseMutationResult } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

// Словарь терминов: записи и поиск по ним.
// Поиск по словарю - отдельная команда, но разбор запроса в Rust общий
// с поиском по карточкам, поэтому поведение совпадает: слова по началу,
// пунктуация вроде arr[0] ищется как есть.

/** Термин словаря: короткая запись и её значение. */
export interface Term {
  id?: number | null;
  term: string;
  meaning: string;
  createdAt: string;
  updatedAt: string;
}

/** Найденный термин: значение и строка вокруг совпадения. */
export interface TermHit {
  id: number;
  term: string;
  meaning: string;
  /** Строка значения вокруг совпадения: её показывает список. */
  snippet: string;
  /** Совпало только значение: находка показывается второй строкой. */
  crumb: boolean;
}

/** Ключи запросов словаря. */
export const termKeys = {
  all: ["terms"] as const,
  list: (query: string) => ["terms", "list", query] as const,
  search: (query: string) => ["terms", "search", query] as const,
};

/** Список терминов. query = "" отдаёт весь словарь по алфавиту. */
export function termsList(query = ""): Promise<Term[]> {
  const trimmed = query.trim();
  return invoke<Term[]>("terms_list", { query: trimmed === "" ? null : trimmed });
}

/** Создание (id = null) или правка термина. Возвращает id записи. */
export function termSave(id: number | null, term: string, meaning: string): Promise<number> {
  return invoke<number>("term_save", { id, term, meaning });
}

/** Удаление термина. */
export function termDelete(id: number): Promise<void> {
  return invoke<void>("term_delete", { id });
}

/** Поиск по словарю тем же запросом, что и по карточкам. */
export function searchTerms(query: string): Promise<TermHit[]> {
  return invoke<TermHit[]>("search_terms", { query });
}

// ---------------------------------------------------------------------------
// Хуки
// ---------------------------------------------------------------------------

/** Словарь под фильтр: пустой фильтр отдаёт весь список. */
export function useTerms(query = "") {
  return useQuery<Term[]>({
    queryKey: termKeys.list(query),
    queryFn: () => termsList(query),
  });
}

/**
 * Поиск по словарю. Пустой запрос отключает выполнение: в это время
 * вкладка словаря показывает обычный список.
 */
export function useTermSearch(query: string) {
  return useQuery<TermHit[]>({
    queryKey: termKeys.search(query),
    queryFn: () => searchTerms(query),
    enabled: query.trim() !== "",
    placeholderData: (previous) => previous,
  });
}

/** После любой правки словаря меняются и список, и результаты поиска. */
function invalidateTerms(queryClient: ReturnType<typeof useQueryClient>) {
  queryClient.invalidateQueries({ queryKey: termKeys.all });
}

/** Создание или правка термина. */
export function useSaveTerm(): UseMutationResult<
  number,
  Error,
  { id: number | null; term: string; meaning: string }
> {
  const queryClient = useQueryClient();
  return useMutation<number, Error, { id: number | null; term: string; meaning: string }>({
    mutationFn: ({ id, term, meaning }) => termSave(id, term, meaning),
    onSuccess: () => invalidateTerms(queryClient),
  });
}

/** Удаление термина. */
export function useDeleteTerm(): UseMutationResult<void, Error, number> {
  const queryClient = useQueryClient();
  return useMutation<void, Error, number>({
    mutationFn: (id) => termDelete(id),
    onSuccess: () => invalidateTerms(queryClient),
  });
}
