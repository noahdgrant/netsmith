// localStorage, where it works. It can be missing or throw (private windows, blocked site data),
// and the page must run without it.

export function load<T>(key: string, fallback: T): T {
  try {
    const text = window.localStorage.getItem(key);
    return text == null ? fallback : (JSON.parse(text) as T);
  } catch {
    return fallback;
  }
}

export function save(key: string, value: unknown) {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Not kept; the page still works.
  }
}
