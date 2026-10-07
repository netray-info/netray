/**
 * Register global keyboard shortcuts that are ignored when focus is in
 * an input, textarea, contenteditable element, or CodeMirror editor.
 *
 * Returns a cleanup function suitable for SolidJS `onCleanup`.
 */
export function createKeyboardShortcuts(
  shortcuts: Record<string, (e: KeyboardEvent) => void>,
): () => void {
  function handler(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const isEditing =
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable ||
      !!target.closest('.cm-editor');

    const fn = shortcuts[e.key];
    if (fn && !isEditing) {
      fn(e);
    }
  }

  document.addEventListener('keydown', handler);
  return () => document.removeEventListener('keydown', handler);
}
