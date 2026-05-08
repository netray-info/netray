// @vitest-environment jsdom
// Tests SDD §10 step 16: BadgeModal closes on Escape key.
import { render, cleanup, fireEvent } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import BadgeModal from './BadgeModal';

afterEach(cleanup);

Object.defineProperty(navigator, 'clipboard', {
  value: { writeText: vi.fn().mockResolvedValue(undefined) },
  configurable: true,
});

describe('BadgeModal — Escape key (SDD §10 step 16)', () => {
  it('calls onClose when Escape is pressed inside the dialog', () => {
    const onClose = vi.fn();
    const { getByRole } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={onClose} />
    ));
    const dialog = getByRole('dialog');
    fireEvent.keyDown(dialog, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });
});
