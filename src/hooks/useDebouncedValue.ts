import { useEffect, useState } from "react";

// Живой поиск: значение доходит до запроса не сразу, а через паузу,
// чтобы не дёргать Rust на каждый символ (раздел 3.4 плана, 200 мс).

/** Возвращает значение, обновляемое после паузы без изменений. */
export function useDebouncedValue<T>(value: T, delayMs = 200): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(timer);
  }, [value, delayMs]);

  return debounced;
}
