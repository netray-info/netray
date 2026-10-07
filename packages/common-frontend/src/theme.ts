import { createSignal, createEffect, onCleanup } from 'solid-js';
import type { Accessor } from 'solid-js';

export type Theme = 'dark' | 'light' | 'system';

export interface ThemeResult {
  theme: Accessor<Theme>;
  resolved: Accessor<'dark' | 'light'>;
  setTheme: (t: Theme) => void;
  toggleTheme: () => void;
}

function getSystemTheme(): 'dark' | 'light' {
  return window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

function resolveTheme(t: Theme): 'dark' | 'light' {
  return t === 'system' ? getSystemTheme() : t;
}

function loadTheme(key: string): Theme | null {
  try {
    const saved = localStorage.getItem(key);
    if (saved === 'light' || saved === 'dark' || saved === 'system') return saved;
  } catch { /* ignore */ }
  return null;
}

function saveTheme(key: string, t: Theme) {
  try { localStorage.setItem(key, t); } catch { /* ignore */ }
}

/**
 * SolidJS primitive for theme management.
 *
 * Reads from localStorage, falls back to system preference via matchMedia,
 * applies data-theme attribute to document.documentElement, and listens
 * for system theme changes.
 *
 * @param storageKey - localStorage key to persist theme choice
 * @param defaultTheme - fallback when nothing is saved (default: 'dark')
 */
export function createTheme(
  storageKey = 'theme',
  defaultTheme: Theme = 'dark',
): ThemeResult {
  const [theme, setThemeSignal] = createSignal<Theme>(loadTheme(storageKey) ?? defaultTheme);
  const [resolved, setResolved] = createSignal<'dark' | 'light'>(resolveTheme(theme()));

  function apply(t: Theme) {
    const r = resolveTheme(t);
    setResolved(r);
    document.documentElement.setAttribute('data-theme', r);
  }

  function setTheme(t: Theme) {
    setThemeSignal(t);
    apply(t);
    saveTheme(storageKey, t);
  }

  function toggleTheme() {
    const next: Theme = theme() === 'system' ? 'dark' : theme() === 'dark' ? 'light' : 'system';
    setTheme(next);
  }

  // Apply on creation
  apply(theme());

  // Listen for system theme changes
  createEffect(() => {
    const current = theme();
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => {
      if (current === 'system') apply('system');
    };
    mq.addEventListener('change', handler);
    onCleanup(() => mq.removeEventListener('change', handler));
  });

  return { theme, resolved, setTheme, toggleTheme };
}
