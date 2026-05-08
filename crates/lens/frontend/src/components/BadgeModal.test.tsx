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
  it('renders badge preview svg', () => {
    const { container } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={vi.fn()} />
    ));
    expect(container.querySelector('svg')).toBeTruthy();
  });

  it('renders Copy HTML and Copy Markdown buttons', () => {
    const { getByText } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={vi.fn()} />
    ));
    expect(getByText('Copy HTML')).toBeTruthy();
    expect(getByText('Copy Markdown')).toBeTruthy();
  });

  it('calls clipboard.writeText when Copy HTML is clicked', async () => {
    const { getByText } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Copy HTML'));
    expect(navigator.clipboard.writeText).toHaveBeenCalled();
  });

  it('calls clipboard.writeText when Copy Markdown is clicked', async () => {
    const { getByText } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Copy Markdown'));
    expect(navigator.clipboard.writeText).toHaveBeenCalled();
  });

  it('calls onClose when the close button is clicked', () => {
    const onClose = vi.fn();
    const { getByLabelText } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={onClose} />
    ));
    fireEvent.click(getByLabelText('Close'));
    expect(onClose).toHaveBeenCalled();
  });

  it('shows "Copied!" after clicking Copy HTML', async () => {
    const { getByText, findByText } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Copy HTML'));
    expect(await findByText('Copied!')).toBeTruthy();
  });

  it('shows "Copied!" after clicking Copy Markdown', async () => {
    const { getByText, findByText } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Copy Markdown'));
    expect(await findByText('Copied!')).toBeTruthy();
  });

  it('renders tab buttons for Badge and Social card', () => {
    const { getByText } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={vi.fn()} />
    ));
    expect(getByText('Badge')).toBeTruthy();
    expect(getByText('Social card')).toBeTruthy();
  });

  it('shows social card panel when Social card tab is clicked', async () => {
    const { getByText } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Social card'));
    expect(getByText('Copy meta tag')).toBeTruthy();
    expect(getByText('Copy URL')).toBeTruthy();
  });

  it('social card panel shows og image', async () => {
    const { getByText, container } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Social card'));
    const img = container.querySelector('.badge-modal__og-preview') as HTMLImageElement;
    expect(img).toBeTruthy();
    expect(img.src).toContain('/og/example.com.png');
  });

  it('copy meta tag writes correct content', async () => {
    const { getByText } = render(() => (
      <BadgeModal domain="example.com" grade="A" onClose={vi.fn()} />
    ));
    fireEvent.click(getByText('Social card'));
    fireEvent.click(getByText('Copy meta tag'));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      expect.stringContaining('og:image'),
    );
  });
});
