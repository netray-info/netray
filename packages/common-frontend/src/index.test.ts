import { describe, it, expect } from 'vitest';
import { createTheme } from './theme.js';
import { createKeyboardShortcuts } from './keyboard.js';
import { createFocusTrap } from './focus-trap.js';

describe('exports', () => {
  it('createTheme is a function', () => {
    expect(typeof createTheme).toBe('function');
  });

  it('createKeyboardShortcuts is a function', () => {
    expect(typeof createKeyboardShortcuts).toBe('function');
  });

  it('createFocusTrap is a function', () => {
    expect(typeof createFocusTrap).toBe('function');
  });
});
