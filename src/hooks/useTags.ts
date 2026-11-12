import { useMutation, useQuery, useQueryClient, type UseMutationResult } from "@tanstack/react-query";
import { cardSetTags, tagList, type TagWithCount } from "../api/tauri";
import { cardKeys, tagKeys } from "./useCards";

// Теги (этап 3): список существующих тегов со счётчиками и точечная
// замена набора тегов у сохранённой карточки.

/** Все существующие теги со счётчиками карточек. */
export function useTags() {
  return useQuery<TagWithCount[]>({
    queryKey: tagKeys.all,
    queryFn: () => tagList(),
  });
}

/** Замена набора тегов карточки. Нужна для точечного сохранения из просмотра. */
export function useSetCardTags(): UseMutationResult<
  void,
  Error,
  { cardId: number; tags: string[] }
> {
  const queryClient = useQueryClient();
  return useMutation<void, Error, { cardId: number; tags: string[] }>({
    mutationFn: ({ cardId, tags }) => cardSetTags(cardId, tags),
    onSuccess: (_result, { cardId }) => {
      queryClient.invalidateQueries({ queryKey: cardKeys.all });
      queryClient.invalidateQueries({ queryKey: cardKeys.detail(cardId) });
      queryClient.invalidateQueries({ queryKey: tagKeys.all });
    },
  });
}

/** Похожий тег: существующий тег, отличающийся от ввода только регистром. */
export interface SimilarTag {
  /** Что ввёл пользователь. */
  input: string;
  /** Существующий тег, который уже есть в базе. */
  existing: string;
}

/**
 * Ищет существующий тег, совпадающий с вводом без учёта регистра.
 * Подсказка мягкая: она не мешает сохранить ввод как есть.
 */
export function findSimilarTag(input: string, known: TagWithCount[]): SimilarTag | null {
  const normalized = input.trim().toLowerCase();
  if (normalized === "") {
    return null;
  }
  const hit = known.find((tag) => tag.name.toLowerCase() === normalized);
  if (hit === undefined || hit.name === input.trim()) {
    return null;
  }
  return { input: input.trim(), existing: hit.name };
}
