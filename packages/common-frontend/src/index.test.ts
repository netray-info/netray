import { describe, it, expect } from 'vitest';
import { createTheme } from './theme.js';
import { createKeyboardShortcuts } from './keyboard.js';
import { createFocusTrap } from './focus-trap.js';
import ThemeToggle from './components/ThemeToggle.js';
import Modal from './components/Modal.js';
import SiteFooter from './components/SiteFooter.js';

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

  it('ThemeToggle is a function', () => {
    expect(typeof ThemeToggle).toBe('function');
  });

  it('Modal is a function', () => {
    expect(typeof Modal).toBe('function');
  });

  it('SiteFooter is a function', () => {
    expect(typeof SiteFooter).toBe('function');
  });
});
