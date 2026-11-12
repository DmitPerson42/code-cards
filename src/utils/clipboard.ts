// Копирование кода в буфер обмена.
// Основной путь - navigator.clipboard (WebView2 его поддерживает),
// запасной - document.execCommand для случаев, когда буфер запрещён политикой окна.

export class ClipboardError extends Error {}

/** Копирует текст в буфер обмена. Бросает ClipboardError при неудаче. */
export async function copyText(text: string): Promise<void> {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      return;
    }
  } catch {
    // Переходим к запасному способу ниже.
  }

  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "readonly");
  area.style.position = "fixed";
  area.style.top = "-1000px";
  document.body.appendChild(area);
  area.select();
  let ok = false;
  try {
    ok = document.execCommand("copy");
  } catch {
    ok = false;
  } finally {
    document.body.removeChild(area);
  }
  if (!ok) {
    throw new ClipboardError("Не удалось скопировать код в буфер обмена");
  }
}
