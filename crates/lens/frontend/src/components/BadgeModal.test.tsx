// @vitest-environment jsdom
import { render, cleanup, fireEvent } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import BadgeModal from './BadgeModal';

afterEach(cleanup);

// Mock navigator.clipboard
Object.defineProperty(navigator, 'clipboard', {
  value: { writeText: vi.fn().mockResolvedValue(undefined) },
  writable: true,
});

describe('BadgeModal', () => {
  it('renders with domain in img alt text', () => {
    const { getByAltText } = render(() => (
      <BadgeModal domain="example.com" onClose={vi.fn()} />
    ));
    expect(getByAltText(/example\.com/)).toBeTruthy();
  });

  it('renders Copy HTML and Copy Markdown buttons', () => {
    const { getByText } = render(() => (
      <BadgeModal domain="example.com" onClose={vi.fn()} />
    ));
    expect(getByText('Copy HTML')).toBeTruthy();
    expect(getByText('Copy Markdown')).toBeTruthy();
  });

  it('calls clipboard.writeText when Copy HTML is clicked', async () => {
    const { getByText } = render(() => (
      <BadgeModal domain="example.com" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Copy HTML'));
    expect(navigator.clipboard.writeText).toHaveBeenCalled();
  });

  it('calls clipboard.writeText when Copy Markdown is clicked', async () => {
    const { getByText } = render(() => (
      <BadgeModal domain="example.com" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Copy Markdown'));
    expect(navigator.clipboard.writeText).toHaveBeenCalled();
  });

  it('calls onClose when the close button is clicked', () => {
    const onClose = vi.fn();
    const { getByLabelText } = render(() => (
      <BadgeModal domain="example.com" onClose={onClose} />
    ));
    fireEvent.click(getByLabelText('Close'));
    expect(onClose).toHaveBeenCalled();
  });

  it('shows "Copied!" after clicking Copy HTML', async () => {
    const { getByText, findByText } = render(() => (
      <BadgeModal domain="example.com" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Copy HTML'));
    expect(await findByText('Copied!')).toBeTruthy();
  });

  it('shows "Copied!" after clicking Copy Markdown', async () => {
    const { getByText, findByText } = render(() => (
      <BadgeModal domain="example.com" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Copy Markdown'));
    expect(await findByText('Copied!')).toBeTruthy();
  });
});
