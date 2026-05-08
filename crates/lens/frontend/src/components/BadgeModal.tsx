import { createSignal } from 'solid-js';
import Modal from '@netray-info/common-frontend/components/Modal';
import { buildBadgeUrl, buildHtmlSnippet, buildMarkdownSnippet, buildOgUrl, buildOgMetaTag } from '../lib/badge';
import GradeBadgePreview from './GradeBadgePreview';

function gradeColor(grade: string): string {
  switch (grade) {
    case 'A+':
    case 'A':  return 'var(--grade-a)';
    case 'B':  return 'var(--grade-b)';
    case 'C':  return 'var(--grade-c)';
    case 'D':  return 'var(--grade-d)';
    default:   return 'var(--grade-f)';
  }
}

type Tab = 'badge' | 'social';

interface Props {
  domain: string;
  grade: string;
  onClose: () => void;
}

export default function BadgeModal(props: Props) {
  const [copied, setCopied] = createSignal<string | null>(null);
  const [activeTab, setActiveTab] = createSignal<Tab>('badge');

  const badgeUrl = () => buildBadgeUrl(window.location.origin, props.domain);
  const ogUrl = () => buildOgUrl(window.location.origin, props.domain);

  async function copy(text: string, label: string) {
    await navigator.clipboard.writeText(text);
    setCopied(label);
    setTimeout(() => setCopied(null), 2000);
  }

  return (
    <Modal open={true} onClose={props.onClose} title="Get the badge">
      <div class="badge-modal">
        <div class="badge-modal__tabs" role="tablist">
          <button
            class={`badge-modal__tab${activeTab() === 'badge' ? ' badge-modal__tab--active' : ''}`}
            role="tab"
            aria-selected={activeTab() === 'badge'}
            type="button"
            onClick={() => setActiveTab('badge')}
          >
            Badge
          </button>
          <button
            class={`badge-modal__tab${activeTab() === 'social' ? ' badge-modal__tab--active' : ''}`}
            role="tab"
            aria-selected={activeTab() === 'social'}
            type="button"
            onClick={() => setActiveTab('social')}
          >
            Social card
          </button>
        </div>

        {activeTab() === 'badge' && (
          <div class="badge-modal__panel">
            <div class="badge-modal__preview">
              <GradeBadgePreview grade={props.grade} color={gradeColor(props.grade)} />
            </div>

            <div class="badge-modal__field">
              <label class="badge-modal__label" for="badge-url">Badge URL</label>
              <input
                id="badge-url"
                class="badge-modal__input"
                type="text"
                readonly
                value={badgeUrl()}
                onClick={(e) => (e.currentTarget as HTMLInputElement).select()}
              />
            </div>

            <div class="badge-modal__actions">
              <button
                class="badge-modal__copy-btn"
                type="button"
                onClick={() => copy(buildHtmlSnippet(badgeUrl(), props.domain, window.location.origin), 'html')}
              >
                {copied() === 'html' ? 'Copied!' : 'Copy HTML'}
              </button>
              <button
                class="badge-modal__copy-btn"
                type="button"
                onClick={() => copy(buildMarkdownSnippet(badgeUrl(), props.domain, window.location.origin), 'md')}
              >
                {copied() === 'md' ? 'Copied!' : 'Copy Markdown'}
              </button>
            </div>
          </div>
        )}

        {activeTab() === 'social' && (
          <div class="badge-modal__panel">
            <div class="badge-modal__preview badge-modal__preview--og">
              <img
                class="badge-modal__og-preview"
                src={ogUrl()}
                alt={`OG card for ${props.domain}`}
                loading="lazy"
              />
            </div>

            <div class="badge-modal__field">
              <label class="badge-modal__label" for="og-url">Card URL</label>
              <input
                id="og-url"
                class="badge-modal__input"
                type="text"
                readonly
                value={ogUrl()}
                onClick={(e) => (e.currentTarget as HTMLInputElement).select()}
              />
            </div>

            <div class="badge-modal__actions">
              <button
                class="badge-modal__copy-btn"
                type="button"
                onClick={() => copy(buildOgMetaTag(ogUrl()), 'meta')}
              >
                {copied() === 'meta' ? 'Copied!' : 'Copy meta tag'}
              </button>
              <button
                class="badge-modal__copy-btn"
                type="button"
                onClick={() => copy(ogUrl(), 'url')}
              >
                {copied() === 'url' ? 'Copied!' : 'Copy URL'}
              </button>
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
}
