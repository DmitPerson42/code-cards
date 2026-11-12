import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import {
  collectionAddCard,
  collectionDelete,
  collectionList,
  collectionSave,
  collectionSetCards,
  type CollectionNode,
} from "../api/tauri";
import { cardKeys, searchKeys } from "./useCards";

// Подборки (этап 5): дерево, создание, переименование, удаление,
// состав и порядок карточек.

export const collectionKeys = {
  all: ["collections"] as const,
};

/** Дерево подборок целиком. */
export function useCollections() {
  return useQuery<CollectionNode[]>({
    queryKey: collectionKeys.all,
    queryFn: () => collectionList(),
  });
}

/** После любой правки подборок меняется и состав списков карточек. */
function invalidateAll(queryClient: ReturnType<typeof useQueryClient>) {
  queryClient.invalidateQueries({ queryKey: collectionKeys.all });
  queryClient.invalidateQueries({ queryKey: cardKeys.all });
  queryClient.invalidateQueries({ queryKey: searchKeys.all });
}

/** Создание или переименование подборки. */
export function useSaveCollection(): UseMutationResult<
  number,
  Error,
  { id: number | null; name: string; parentId: number | null }
> {
  const queryClient = useQueryClient();
  return useMutation<
    number,
    Error,
    { id: number | null; name: string; parentId: number | null }
  >({
    mutationFn: ({ id, name, parentId }) => collectionSave(id, name, parentId),
    onSuccess: () => invalidateAll(queryClient),
  });
}

/** Удаление подборки вместе с вложенными. */
export function useDeleteCollection(): UseMutationResult<void, Error, number> {
  const queryClient = useQueryClient();
  return useMutation<void, Error, number>({
    mutationFn: (id) => collectionDelete(id),
    onSuccess: () => invalidateAll(queryClient),
  });
}

/** Полный состав и порядок карточек подборки. */
export function useSetCollectionCards(): UseMutationResult<
  void,
  Error,
  { collectionId: number; cardIds: number[] }
> {
  const queryClient = useQueryClient();
  return useMutation<void, Error, { collectionId: number; cardIds: number[] }>({
    mutationFn: ({ collectionId, cardIds }) => collectionSetCards(collectionId, cardIds),
    onSuccess: () => invalidateAll(queryClient),
  });
}

/** Добавление одной карточки в конец подборки. */
export function useAddCardToCollection(): UseMutationResult<
  void,
  Error,
  { collectionId: number; cardId: number }
> {
  const queryClient = useQueryClient();
  return useMutation<void, Error, { collectionId: number; cardId: number }>({
    mutationFn: ({ collectionId, cardId }) => collectionAddCard(collectionId, cardId),
    onSuccess: () => invalidateAll(queryClient),
  });
}

/** Плоский список подборок в порядке обхода дерева: нужно для меню "В подборку". */
export function flattenCollections(
  nodes: CollectionNode[],
  depth = 0,
): Array<{ node: CollectionNode; depth: number }> {
  const result: Array<{ node: CollectionNode; depth: number }> = [];
  for (const node of nodes) {
    result.push({ node, depth });
    result.push(...flattenCollections(node.children, depth + 1));
  }
  return result;
}
