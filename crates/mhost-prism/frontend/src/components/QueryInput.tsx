import { onMount, onCleanup, Show, For, createSignal } from 'solid-js';
import { EditorView, keymap, placeholder as cmPlaceholder, ViewPlugin, Decoration, type DecorationSet } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { acceptCompletion, autocompletion, startCompletion, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { tokenize, TokenType } from '../lib/tokenizer';

// ---------------------------------------------------------------------------
// Autocomplete data
// ---------------------------------------------------------------------------

const RECORD_TYPES = [
  { label: 'A', detail: 'IPv4 address' },
  { label: 'AAAA', detail: 'IPv6 address' },
  { label: 'MX', detail: 'Mail exchange' },
  { label: 'TXT', detail: 'Text record' },
  { label: 'NS', detail: 'Name server' },
  { label: 'SOA', detail: 'Start of authority' },
  { label: 'CNAME', detail: 'Canonical name' },
  { label: 'CAA', detail: 'Certification authority' },
  { label: 'SRV', detail: 'Service locator' },
  { label: 'PTR', detail: 'Pointer (reverse DNS)' },
  { label: 'HTTPS', detail: 'HTTPS service binding' },
  { label: 'SVCB', detail: 'Service binding' },
  { label: 'SSHFP', detail: 'SSH fingerprint' },
  { label: 'TLSA', detail: 'TLS association (DANE)' },
  { label: 'NAPTR', detail: 'Naming authority pointer' },
  { label: 'HINFO', detail: 'Host information' },
  { label: 'OPENPGPKEY', detail: 'OpenPGP public key' },
  { label: 'DNSKEY', detail: 'DNSSEC key' },
  { label: 'DS', detail: 'Delegation signer' },
];

const SERVERS = [
  { label: '@cloudflare', detail: '1.1.1.1 / 1.0.0.1' },
  { label: '@google', detail: '8.8.8.8 / 8.8.4.4' },
  { label: '@quad9', detail: '9.9.9.9' },
  { label: '@mullvad', detail: 'Mullvad DNS' },
  { label: '@wikimedia', detail: 'Wikimedia DNS' },
  { label: '@dns4eu', detail: 'DNS4EU' },
  { label: '@system', detail: 'System resolvers (/etc/resolv.conf)' },
];

const FLAGS = [
  { label: '+udp', detail: 'UDP transport (default)' },
  { label: '+tcp', detail: 'TCP transport' },
  { label: '+tls', detail: 'DNS-over-TLS' },
  { label: '+https', detail: 'DNS-over-HTTPS' },
  { label: '+dnssec', detail: 'DNSSEC validation' },
  { label: '+check', detail: 'DNS health check (lint all record types)' },
  { label: '+trace', detail: 'Delegation trace (walk root → authoritative)' },
  { label: '+compare', detail: 'Transport comparison (UDP/TCP/TLS/HTTPS)' },
  { label: '+auth', detail: 'Authoritative vs recursive comparison' },
];

function prismCompletions(context: CompletionContext): CompletionResult | null {
  // Match the current word being typed. We look for word chars, @, or +.
  const word = context.matchBefore(/[@+]?\w*/);
  if (!word || (word.from === word.to && !context.explicit)) return null;

  const text = word.text;
  const options: Array<{ label: string; detail: string; type: string }> = [];

  if (text.startsWith('@')) {
    for (const s of SERVERS) {
      options.push({ ...s, type: 'variable' });
    }
  } else if (text.startsWith('+')) {
    for (const f of FLAGS) {
      options.push({ ...f, type: 'keyword' });
    }
  } else {
    // Offer record types (only after the first token — the domain)
    const fullLine = context.state.doc.toString();
    const beforeCursor = fullLine.slice(0, word.from);
    const hasDomain = /\S/.test(beforeCursor);
    if (hasDomain) {
      for (const rt of RECORD_TYPES) {
        options.push({ ...rt, type: 'type' });
      }
    }
    // Also offer servers and flags after domain
    if (hasDomain) {
      for (const s of SERVERS) {
        options.push({ ...s, type: 'variable' });
      }
      for (const f of FLAGS) {
        options.push({ ...f, type: 'keyword' });
      }
    }
  }

  return {
    from: word.from,
    options,
    validFor: /^[@+]?\w*$/,
  };
}

// ---------------------------------------------------------------------------
// Syntax highlighting via ViewPlugin + Decorations
// ---------------------------------------------------------------------------

const tokenDecorations: Record<string, Decoration> = {
  [TokenType.Domain]: Decoration.mark({ class: 'cm-domain' }),
  [TokenType.RecordType]: Decoration.mark({ class: 'cm-record-type' }),
  [TokenType.Server]: Decoration.mark({ class: 'cm-server' }),
  [TokenType.Flag]: Decoration.mark({ class: 'cm-flag' }),
  [TokenType.Unknown]: Decoration.mark({ class: 'cm-unknown' }),
};

const highlightPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = this.buildDecorations(view);
    }

    update(update: { docChanged: boolean; view: EditorView }) {
      if (update.docChanged) {
        this.decorations = this.buildDecorations(update.view);
      }
    }

    buildDecorations(view: EditorView): DecorationSet {
      const doc = view.state.doc.toString();
      const tokens = tokenize(doc);
      const ranges = tokens
        .map((t) => {
          const deco = tokenDecorations[t.type];
          return deco ? deco.range(t.from, t.to) : null;
        })
        .filter((r): r is NonNullable<typeof r> => r !== null);

      return Decoration.set(ranges, true);
    }
  },
  {
    decorations: (v) => v.decorations,
  },
);

// ---------------------------------------------------------------------------
// Editor theme (uses CSS custom properties — works for both light and dark)
// ---------------------------------------------------------------------------

const editorTheme = EditorView.theme(
  {
    '&': {
      backgroundColor: 'var(--bg-secondary)',
      color: 'var(--text-primary)',
      fontSize: '15px',
      fontFamily: 'var(--font-mono)',
      borderRadius: 'var(--radius)',
      border: '1px solid var(--border)',
      transition: 'border-color var(--transition)',
    },
    '&.cm-focused': {
      outline: 'none',
      borderColor: 'var(--accent)',
    },
    '.cm-scroller': {
      overflow: 'hidden',
      lineHeight: '1.6',
      padding: '8px 28px 8px 12px',
    },
    '.cm-content': {
      caretColor: 'var(--accent)',
      padding: '0',
      minHeight: 'auto',
    },
    '.cm-line': {
      padding: '0',
    },
    '.cm-cursor': {
      borderLeftColor: 'var(--accent)',
      borderLeftWidth: '2px',
    },
    '.cm-selectionBackground': {
      backgroundColor: 'var(--selection-bg) !important',
    },
    '&.cm-focused .cm-selectionBackground': {
      backgroundColor: 'var(--selection-bg-focused) !important',
    },
    '.cm-placeholder': {
      color: 'var(--text-muted)',
      fontStyle: 'italic',
    },
    // Autocomplete panel
    '.cm-tooltip': {
      backgroundColor: 'var(--bg-secondary)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius)',
    },
    '.cm-tooltip-autocomplete': {
      '& > ul': {
        fontFamily: 'var(--font-mono)',
        fontSize: '13px',
      },
      '& > ul > li': {
        padding: '4px 8px',
        color: 'var(--text-primary)',
      },
      '& > ul > li[aria-selected]': {
        backgroundColor: 'var(--bg-tertiary)',
        color: 'var(--text-primary)',
      },
    },
    '.cm-completionLabel': {
      color: 'var(--accent)',
    },
    '.cm-completionDetail': {
      color: 'var(--text-secondary)',
      marginLeft: '8px',
      fontStyle: 'italic',
    },
    // Token highlight classes
    '.cm-domain': {
      color: 'var(--text-primary)',
      fontWeight: '600',
    },
    '.cm-record-type': {
      color: 'var(--rt-a)',
      fontWeight: '600',
    },
    '.cm-server': {
      color: 'var(--rt-ns)',
    },
    '.cm-flag': {
      color: 'var(--rt-soa)',
    },
    '.cm-unknown': {
      color: 'var(--text-secondary)',
      textDecoration: 'underline wavy var(--warning)',
    },
  },
  { dark: true },
);

// ---------------------------------------------------------------------------
// Single-line enforcement: filter out newlines
// ---------------------------------------------------------------------------

const singleLine = EditorState.transactionFilter.of((tr) => {
  if (!tr.docChanged) return tr;
  const newDoc = tr.newDoc.toString();
  if (newDoc.includes('\n')) {
    // Replace newlines with spaces
    return {
      changes: { from: 0, to: tr.startState.doc.length, insert: newDoc.replace(/\n/g, ' ') },
      sequential: true,
    };
  }
  return tr;
});

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface QueryInputProps {
  onSubmit: (query: string) => void;
  initialValue?: string;
  history: string[];
  disabled?: boolean;
  onReset?: () => void;
  onReady?: (api: { focus: () => void; clear: () => void; setValue: (v: string) => void }) => void;
  shareLabel?: string;
  onShare?: () => void;
}

export function QueryInput(props: QueryInputProps) {
  let containerRef: HTMLDivElement | undefined;
  let wrapRef: HTMLDivElement | undefined;
  let view: EditorView | undefined;
  let historyIndex = -1;
  let savedInput = '';
  const [showHistory, setShowHistory] = createSignal(false);
  const [historyHighlight, setHistoryHighlight] = createSignal(-1);

  onMount(() => {
    if (!containerRef) return;

    const submitKeymap = keymap.of([
      {
        key: 'Escape',
        run: () => {
          if (showHistory()) {
            setShowHistory(false);
            setHistoryHighlight(-1);
            return true;
          }
          return false;
        },
      },
      {
        key: 'Enter',
        run: (v) => {
          if (props.disabled) return true;
          // If history dropdown is open with a highlighted item, select it
          if (showHistory() && historyHighlight() >= 0) {
            const item = props.history[historyHighlight()];
            if (item) {
              v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: item } });
              setShowHistory(false);
              setHistoryHighlight(-1);
              historyIndex = -1;
              props.onSubmit(item);
              return true;
            }
          }
          setShowHistory(false);
          const query = v.state.doc.toString().trim();
          if (query) {
            historyIndex = -1;
            props.onSubmit(query);
          }
          return true;
        },
      },
      {
        // Trap Tab: accept autocomplete if open, otherwise trigger it.
        // Never let Tab leave the single-line editor.
        key: 'Tab',
        run: (v) => {
          if (acceptCompletion(v)) return true;
          startCompletion(v);
          return true;
        },
      },
    ]);

    // History navigation via Up/Down arrows.
    // Placed after autocompletion so autocomplete gets first dibs when its
    // panel is open. When the panel is closed, autocomplete's keymap passes
    // through and our handler fires.
    const historyKeymap = keymap.of([
      {
        key: 'ArrowUp',
        run: (v) => {
          const hist = props.history;
          if (hist.length === 0) return false;
          // If dropdown is visible, navigate highlight
          if (showHistory()) {
            setHistoryHighlight((h) => Math.min(h + 1, hist.length - 1));
            return true;
          }
          if (historyIndex === -1) {
            savedInput = v.state.doc.toString();
          }
          if (historyIndex < hist.length - 1) {
            historyIndex++;
            v.dispatch({
              changes: { from: 0, to: v.state.doc.length, insert: hist[historyIndex] },
            });
          }
          return true;
        },
      },
      {
        key: 'ArrowDown',
        run: (v) => {
          // If dropdown is visible, navigate highlight
          if (showHistory()) {
            const next = historyHighlight() - 1;
            if (next < -1) return true;
            setHistoryHighlight(next);
            if (next === -1) setShowHistory(false);
            return true;
          }
          if (historyIndex <= -1) return false;
          if (historyIndex > 0) {
            historyIndex--;
            v.dispatch({
              changes: { from: 0, to: v.state.doc.length, insert: props.history[historyIndex] },
            });
          } else {
            historyIndex = -1;
            v.dispatch({
              changes: { from: 0, to: v.state.doc.length, insert: savedInput },
            });
          }
          return true;
        },
      },
    ]);

    const state = EditorState.create({
      doc: props.initialValue ?? '',
      extensions: [
        submitKeymap,
        cmPlaceholder('example.com A AAAA @google +tls'),
        editorTheme,
        highlightPlugin,
        autocompletion({
          override: [prismCompletions],
          activateOnTyping: true,
          defaultKeymap: true,
        }),
        historyKeymap,
        singleLine,
        EditorView.lineWrapping,
      ],
    });

    view = new EditorView({
      state,
      parent: containerRef,
    });

    // Reset history index when user types; hide dropdown when typing.
    view.contentDOM.addEventListener('beforeinput', () => {
      historyIndex = -1;
      setShowHistory(false);
    });

    // Show history dropdown on focus (if there's history and input is empty).
    view.contentDOM.addEventListener('focus', () => {
      if (props.history.length > 0 && view!.state.doc.toString().trim() === '') {
        setHistoryHighlight(-1);
        setShowHistory(true);
      }
    });

    // Hide on outside click (defer so click on dropdown item fires first).
    const onPointerDown = (e: PointerEvent) => {
      if (wrapRef && !wrapRef.contains(e.target as Node)) {
        setShowHistory(false);
      }
    };
    document.addEventListener('pointerdown', onPointerDown);
    onCleanup(() => document.removeEventListener('pointerdown', onPointerDown));

    props.onReady?.({
      focus: () => view?.focus(),
      clear: () => {
        if (!view) return;
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: '' } });
      },
      setValue: (v: string) => {
        if (!view) return;
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } });
        view.focus();
      },
    });
  });

  onCleanup(() => {
    view?.destroy();
  });

  const handleSubmitClick = () => {
    if (!view) return;
    setShowHistory(false);
    const query = view.state.doc.toString().trim();
    if (query) {
      historyIndex = -1;
      props.onSubmit(query);
    }
  };

  const selectHistoryItem = (item: string) => {
    if (!view) return;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: item } });
    setShowHistory(false);
    setHistoryHighlight(-1);
    historyIndex = -1;
    props.onSubmit(item);
  };

  return (
    <div class="query-bar">
      <div class="query-editor-wrap" ref={wrapRef}>
        <div class="query-editor" ref={containerRef} aria-label="DNS query" />
        <Show when={props.onReset}>
          <button
            class="query-clear-btn"
            onClick={props.onReset}
            title="Clear"
            tabIndex={-1}
          >
            &times;
          </button>
        </Show>
        <Show when={showHistory() && props.history.length > 0}>
          <div class="history-dropdown">
            <For each={props.history}>
              {(item, i) => (
                <button
                  class={`history-item${historyHighlight() === i() ? ' history-item--active' : ''}`}
                  onPointerDown={(e) => {
                    e.preventDefault();
                    selectHistoryItem(item);
                  }}
                >
                  {item}
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>
      <Show when={props.shareLabel}>
        <button
          class="share-btn"
          onClick={props.onShare}
          title={props.shareLabel === 'Share' ? 'Copy shareable permalink' : props.shareLabel!}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" /><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" /></svg>
        </button>
      </Show>
      <button
        class="query-button"
        onClick={handleSubmitClick}
        disabled={props.disabled}
        aria-busy={props.disabled ? 'true' : 'false'}
        title="Run query (Enter)"
      >
        Query
      </button>
    </div>
  );
}
