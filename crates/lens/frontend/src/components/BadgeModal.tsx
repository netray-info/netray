import { createSignal } from 'solid-js';
import Modal from '@netray-info/common-frontend/components/Modal';
import { buildBadgeUrl, buildHtmlSnippet, buildMarkdownSnippet } from '../lib/badge';
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

interface Props {
  domain: string;
  grade: string;
  onClose: () => void;
}

export default function BadgeModal(props: Props) {
  const [copied, setCopied] = createSignal<string | null>(null);

  const badgeUrl = () => buildBadgeUrl(window.location.origin, props.domain);

  async function copy(text: string, label: string) {
    await navigator.clipboard.writeText(text);
    setCopied(label);
    setTimeout(() => setCopied(null), 2000);
  }

  return (
    <Modal open={true} onClose={props.onClose} title="Get the badge">
      <div class="badge-modal">
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
    </Modal>
  );
}
