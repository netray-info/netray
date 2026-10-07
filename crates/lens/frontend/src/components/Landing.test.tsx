// @vitest-environment jsdom
import { render, cleanup } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import Landing from './Landing';

afterEach(cleanup);

// Hardcoded fallbacks live in Landing.tsx and double as the safety net when
// /api/meta is slow or fails. These tests assert both branches of that ??
// chain — meta-driven render and fallback render — so a future SDD-defaults
// drift between backend and frontend surfaces here rather than in production.

const FALLBACK_HEADING = 'How healthy is your domain?';
const FALLBACK_TRUST = 'No account · No ads · Open source · Self-hostable';

describe('Landing', () => {
  it('renders meta.site values when present', () => {
    const { getByText } = render(() => (
      <Landing
        site={{
          hero_heading: 'Is your domain healthy?',
          hero_subheading: 'Custom subheading.',
          status_pill: 'self-hosted build',
          example_domains: ['example.com', 'github.com'],
          trust_strip: 'Custom trust line.',
        }}
        onExampleClick={vi.fn()}
      />
    ));

    expect(getByText('Is your domain healthy?')).toBeTruthy();
    expect(getByText('Custom subheading.')).toBeTruthy();
    expect(getByText('self-hosted build')).toBeTruthy();
    expect(getByText('example.com')).toBeTruthy();
    expect(getByText('github.com')).toBeTruthy();
    expect(getByText('Custom trust line.')).toBeTruthy();
  });

  it('falls back to hardcoded defaults when site prop is undefined', () => {
    const { getByText } = render(() => (
      <Landing site={undefined} onExampleClick={vi.fn()} />
    ));

    expect(getByText(FALLBACK_HEADING)).toBeTruthy();
    expect(getByText(FALLBACK_TRUST)).toBeTruthy();
    expect(getByText('netray.info')).toBeTruthy();
  });

  it('uses defaults for missing fields and overrides for present fields', () => {
    const { getByText, queryByText } = render(() => (
      <Landing
        site={{ hero_heading: 'Custom only.' }}
        onExampleClick={vi.fn()}
      />
    ));

    expect(getByText('Custom only.')).toBeTruthy();
    expect(queryByText(FALLBACK_HEADING)).toBeNull();
    // trust_strip not provided — fallback fires.
    expect(getByText(FALLBACK_TRUST)).toBeTruthy();
  });

  it('invokes onExampleClick with the chip domain', () => {
    const onExampleClick = vi.fn();
    const { getByText } = render(() => (
      <Landing
        site={{ example_domains: ['example.com'] }}
        onExampleClick={onExampleClick}
      />
    ));

    (getByText('example.com') as HTMLButtonElement).click();
    expect(onExampleClick).toHaveBeenCalledWith('example.com');
  });
});
