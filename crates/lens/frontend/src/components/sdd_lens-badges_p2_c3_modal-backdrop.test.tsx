// @vitest-environment jsdom
// Tests SDD §10 step 16: BadgeModal closes on backdrop click.
import { render, cleanup, fireEvent } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import BadgeModal from './BadgeModal';

afterEach(cleanup);

Object.defineProperty(navigator, 'clipboard', {
  value: { writeText: vi.fn().mockResolvedValue(undefined) },
  configurable: true,
});

describe('BadgeModal — backdrop click (SDD §10 step 16)', () => {
  it('calls onClose when the backdrop overlay is clicked', () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={onClose} />
    ));
    const overlay = container.querySelector('.modal-overlay');
    expect(overlay).not.toBeNull();
    fireEvent.click(overlay!);
    expect(onClose).toHaveBeenCalled();
  });
});
