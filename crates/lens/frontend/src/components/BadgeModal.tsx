import { Show, createSignal } from 'solid-js';
import Modal from '@netray-info/common-frontend/components/Modal';
import {
  buildBadgeUrl,
  buildHtmlSnippet,
  buildMarkdownSnippet,
  buildOgUrl,
  buildOgMetaTag,
  buildOgMarkdown,
  buildRerunUrl,
  buildRerunMarkdown,
} from '../lib/badge';
import { buildSnapshotUrl, buildShareSnippets } from '../lib/snapshot';
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

type Tab = 'badge' | 'social' | 'snapshot' | 'rerun';

interface Props {
  domain: string;
  grade: string;
  snapshotId?: string | null;
  onClose: () => void;
}

export default function BadgeModal(props: Props) {
  const [copied, setCopied] = createSignal<string | null>(null);
  const [activeTab, setActiveTab] = createSignal<Tab>('badge');

  const modalTitle = () => {
    switch (activeTab()) {
      case 'badge':    return 'Embed badge';
      case 'social':   return 'Social card';
      case 'snapshot': return 'Snapshot';
      case 'rerun':    return 'Re-run link';
    }
  };

  const badgeUrl = () => buildBadgeUrl(window.location.origin, props.domain);
  const ogUrl = () => buildOgUrl(window.location.origin, props.domain);
  const snapshotUrl = () =>
    props.snapshotId ? buildSnapshotUrl(window.location.origin, props.snapshotId) : null;
  const rerunUrl = () => buildRerunUrl(window.location.origin, props.domain);

  async function copy(text: string, label: string) {
    await navigator.clipboard.writeText(text);
    setCopied(label);
    setTimeout(() => setCopied(null), 2000);
  }

  return (
    <Modal open={true} onClose={props.onClose} title={modalTitle()}>
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
          <button
            class={`badge-modal__tab${activeTab() === 'snapshot' ? ' badge-modal__tab--active' : ''}`}
            role="tab"
            aria-selected={activeTab() === 'snapshot'}
            type="button"
            onClick={() => setActiveTab('snapshot')}
          >
            Snapshot
          </button>
          <button
            class={`badge-modal__tab${activeTab() === 'rerun' ? ' badge-modal__tab--active' : ''}`}
            role="tab"
            aria-selected={activeTab() === 'rerun'}
            type="button"
            onClick={() => setActiveTab('rerun')}
          >
            Re-run link
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
              <button
                class="badge-modal__copy-btn"
                type="button"
                onClick={() => copy(buildOgMarkdown(window.location.origin, props.domain), 'og-md')}
              >
                {copied() === 'og-md' ? 'Copied!' : 'Copy Markdown'}
              </button>
            </div>
          </div>
        )}

        {activeTab() === 'snapshot' && (
          <div class="badge-modal__panel">
            <Show
              when={snapshotUrl()}
              fallback={
                <p class="badge-modal__share-unavailable">
                  Snapshot is not available for this check.
                </p>
              }
            >
              {(url) => {
                const snippets = () =>
                  buildShareSnippets(props.domain, props.snapshotId!, window.location.origin);
                return (
                  <>
                    <p class="badge-modal__share-desc">
                      Permanent link to this exact result. Anyone visiting sees the same grades and findings.
                    </p>
                    <div class="badge-modal__field">
                      <div class="badge-modal__input-row">
                        <input
                          id="share-url"
                          class="badge-modal__input"
                          type="text"
                          readonly
                          value={url()}
                          onClick={(e) => (e.currentTarget as HTMLInputElement).select()}
                        />
                        <a
                          class="ext-link"
                          href={url()}
                          target="_blank"
                          rel="noopener noreferrer"
                        >
                          preview ↗
                        </a>
                      </div>
                    </div>
                    <div class="badge-modal__actions">
                      <button
                        class="badge-modal__copy-btn"
                        type="button"
                        onClick={() => copy(snippets().plainUrl, 'share-url')}
                      >
                        {copied() === 'share-url' ? 'Copied!' : 'Copy URL'}
                      </button>
                      <button
                        class="badge-modal__copy-btn"
                        type="button"
                        onClick={() => copy(snippets().markdown, 'share-md')}
                      >
                        {copied() === 'share-md' ? 'Copied!' : 'Copy Markdown'}
                      </button>
                    </div>
                  </>
                );
              }}
            </Show>
          </div>
        )}

        {activeTab() === 'rerun' && (
          <div class="badge-modal__panel">
            <p class="badge-modal__share-desc">
              Re-runs the check from scratch. Use for monitoring, bookmarks, or to invite others to verify.
            </p>
            <div class="badge-modal__field">
              <div class="badge-modal__input-row">
                <input
                  id="rerun-url"
                  class="badge-modal__input"
                  type="text"
                  readonly
                  value={rerunUrl()}
                  onClick={(e) => (e.currentTarget as HTMLInputElement).select()}
                />
              </div>
            </div>
            <div class="badge-modal__actions">
              <button
                class="badge-modal__copy-btn"
                type="button"
                onClick={() => copy(rerunUrl(), 'rerun-url')}
              >
                {copied() === 'rerun-url' ? 'Copied!' : 'Copy URL'}
              </button>
              <button
                class="badge-modal__copy-btn"
                type="button"
                onClick={() => copy(buildRerunMarkdown(window.location.origin, props.domain), 'rerun-md')}
              >
                {copied() === 'rerun-md' ? 'Copied!' : 'Copy Markdown'}
              </button>
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
}
