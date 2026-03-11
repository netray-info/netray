const FOCUSABLE_SELECTOR =
  'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

export interface FocusTrapResult {
  activate: () => void;
  deactivate: () => void;
}

/**
 * Creates a focus trap within a container element.
 *
 * - Traps Tab/Shift+Tab within the container
 * - Calls `onEscape` when Escape is pressed
 * - Saves and restores focus on activate/deactivate
 *
 * @param containerRef - accessor returning the container element (or undefined)
 * @param onEscape - callback when Escape is pressed inside the trap
 */
export function createFocusTrap(
  containerRef: () => HTMLElement | undefined,
  onEscape?: () => void,
): FocusTrapResult {
  let preActiveFocus: HTMLElement | null = null;
  let handler: ((e: KeyboardEvent) => void) | null = null;

  function activate() {
    preActiveFocus = document.activeElement as HTMLElement | null;
    const container = containerRef();
    if (!container) return;

    handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onEscape?.();
        return;
      }

      if (e.key !== 'Tab') return;

      const focusable = container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
      if (focusable.length === 0) return;

      const first = focusable[0];
      const last = focusable[focusable.length - 1];

      if (e.shiftKey) {
        if (document.activeElement === first) {
          e.preventDefault();
          last.focus();
        }
      } else {
        if (document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    };

    container.addEventListener('keydown', handler);

    // Focus first focusable element
    requestAnimationFrame(() => {
      const focusable = container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
      if (focusable.length > 0) focusable[0].focus();
    });
  }

  function deactivate() {
    const container = containerRef();
    if (container && handler) {
      container.removeEventListener('keydown', handler);
      handler = null;
    }
    preActiveFocus?.focus();
    preActiveFocus = null;
  }

  return { activate, deactivate };
}
