import { describe, it, expect } from 'vitest';
import { createTheme } from './theme.js';
import { createKeyboardShortcuts } from './keyboard.js';
import { createFocusTrap } from './focus-trap.js';
import ThemeToggle from './components/ThemeToggle.js';
import Modal from './components/Modal.js';
import SiteFooter from './components/SiteFooter.js';
import SuiteNav from './components/SuiteNav.js';
import { createQueryHistory } from './history.js';
import { fetchWithTimeout } from './api.js';

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

  it('SuiteNav is a function', () => {
    expect(typeof SuiteNav).toBe('function');
  });

  it('createQueryHistory is a function', () => {
    expect(typeof createQueryHistory).toBe('function');
  });

  it('fetchWithTimeout is a function', () => {
    expect(typeof fetchWithTimeout).toBe('function');
  });
});
