# Frontend Rules for New netray.info Tools

Derived from analysis of ifconfig-rs, mhost-prism, tlsight, netray-common-frontend, and lens frontends.
Validated against live UI via Playwright scan of all four tools.

---

## 0. Design Philosophy

The netray.info landing page is the **target aesthetic**: vibrant, dark-by-default, colorful where the data calls for it, professional but not sterile. Tools should move toward that energy.

- **Colorful is correct.** Semantic colors (pass/warn/fail, DNS record types, IP classification badges) should be expressive and distinct. Don't reduce color to improve "coherence" — lack of color is not coherence, it's blandness.
- **Dark-mode by default, always.** The dark palette is the identity of the suite. Light mode is an opt-in, not the design foundation.
- **Vibrant accents.** Use `--accent` (`#00d4ff`) on active states, focused inputs, active chips, and key results. The cyan-to-purple gradient on the `.logo` heading is a suite signature — don't remove it.
- **Engaging idle states.** Empty states and landing cards should be inviting, not blank. Show example queries, a brief description, mode cards if the tool has distinct modes.
- **Consistency through shared primitives, not sameness.** Each tool has a different domain and may use different colors to encode different semantics. What must be consistent: button shape, nav, footer, chip style, error style, font stack, and spacing scale.

---

## 1. Directory & File Layout

```
tool-name/frontend/
├── src/
│   ├── index.tsx              # Entry point: ErrorBoundary wraps App
│   ├── App.tsx                # Main state, routing, layout
│   ├── components/            # PascalCase.tsx
│   ├── lib/                   # camelCase.ts (api, types, history, utils)
│   └── styles/
│       └── global.css         # Imports common-frontend stylesheets + tool overrides
├── vite.config.ts
├── vitest.config.ts
├── tsconfig.json
├── package.json
└── .npmrc                     # GitHub Packages auth
```

- No barrel `index.ts` files — import files directly.
- Components: PascalCase.tsx. Utilities: camelCase.ts. Styles: lowercase.css.

---

## 2. Build & Tooling

- **Bundler**: Vite + `vite-plugin-solid`.
- **Build script**: `tsc && vite build` — type-check before bundling.
- **Dev proxy**: `/api` → `http://127.0.0.1:808x`; use the next available port after 8081.
- **Version injection**: `vite.config.ts` reads `Cargo.toml` and sets `define.__APP_VERSION__`.
- **Test config**: Separate `vitest.config.ts`; use `happy-dom` for component tests, `node` for utility tests.
- **`.npmrc`**: Required for GitHub Packages auth; CI must inject `NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` on every `npm ci` step.

### tsconfig.json

```json
{
  "compilerOptions": {
    "strict": true,
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "esModuleInterop": true,
    "jsx": "preserve",
    "jsxImportSource": "solid-js",
    "types": ["vite/client"],
    "noEmit": true,
    "isolatedModules": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "skipLibCheck": true
  },
  "include": ["src"]
}
```

---

## 3. Consuming netray-common-frontend

Every tool must consume all of these — no exceptions.

### Stylesheets (in `global.css`)

```css
@import '@netray-info/common-frontend/styles/theme.css';
@import '@netray-info/common-frontend/styles/reset.css';
@import '@netray-info/common-frontend/styles/layout.css';
@import '@netray-info/common-frontend/styles/components.css';
```

### Theme

```ts
import { createTheme } from '@netray-info/common-frontend/theme';
const themeResult = createTheme('toolname_theme', 'system');
```

- The storage key **must** be `toolname_theme` (e.g. `ifconfig_theme`, `prism_theme`, `lens_theme`). Never use a generic `'theme'` key — it will collide across tools if a user opens multiple.
- Use `<ThemeToggle theme={themeResult} />` — never roll a custom toggle.

### Components

| Component | When to use |
|-----------|-------------|
| `<ThemeToggle>` | Always — in the header toolbar |
| `<SiteFooter>` | Always — at the bottom of the page |
| `<SuiteNav>` | Always — at the top of the page |
| `<Modal>` | All dialogs — provides focus trap |
| `<CrossLink>` | Any deep-link to another tool in the suite |

### Shared CSS classes

| Class | Source | Usage |
|-------|--------|-------|
| `.btn-primary` | `components.css` | **All** primary action buttons — submit, look up, query, inspect |
| `.filter-toggle` / `.filter-toggle--active` | `components.css` | Toggle chips (view options, filters) |
| `.header` / `.logo` / `.header-btn` / `.header-actions` | `components.css` | Tool header layout |
| `.spinner` | `components.css` | Loading spinners — do not redefine |
| `.results-summary` / `.results-summary-item` | `components.css` | Result count / status line |

**Do not redefine any shared class in tool-level `global.css`** — override only via CSS custom properties if necessary.

### Utilities

- `storageGet/storageSet/storageRemove` from `./storage` — never raw `localStorage`.
- `createKeyboardShortcuts()` from `./keyboard` — skips INPUT/TEXTAREA/contenteditable/.cm-editor automatically.
- `copyToClipboard()` / `downloadFile()` from `./utils`.

---

## 4. Suite Navigation

- Use the `SuiteNav` component from `netray-common-frontend`.
- HTML structure:

```tsx
<SuiteNav current="ip" meta={meta()?.ecosystem} />
```

- Labels are **uppercase short names**: IP, DNS, TLS, LENS. Current tool gets `suite-nav__link--active` + `aria-current="page"`.
- BEM classes: `suite-nav`, `suite-nav__brand`, `suite-nav__sep`, `suite-nav__link`, `suite-nav__link--active`.
- All URLs come from `meta.ecosystem.*_base_url` — no hardcoded production URLs in source. Fall back to `https://*.netray.info` if meta is unavailable.
- **Do not redefine `.suite-nav*` CSS** in tool-level `global.css`. The `<SuiteNav>` component owns its styles via an inline `<style>` tag. Any local redefinition will conflict.

---

## 5. Meta Endpoint Integration

Every tool must fetch `/api/meta` on mount:

```ts
onMount(() => {
  fetchMeta()
    .then(m => {
      setMeta(m);
      if (m?.site_name) document.title = m.site_name;
    })
    .catch(() => {}); // never block on meta failure
});
```

- `fetchMeta()` must return `null` on any failure — never throw.
- Meta failure must not block tool functionality or navigation.
- Cross-tool deep links use `meta().ecosystem.*_base_url` + `encodeURIComponent()`.

---

## 6. Page Structure & Layout

- `<h1>` = tool name only (e.g. `tlsight`); tagline as an adjacent `<span>` or `<p>`.
- `document.title` set from `meta.site_name` on mount; fall back to `location.hostname`.
- Required landmarks: `<nav>` (suite nav), `<main>` (input + results), `<footer>` (SiteFooter).
- Include a visually-hidden "Skip to results" skip link (revealed on focus).
- Include a `?` help button (32×32px min) in the header toolbar that opens a `<Modal>`.

### Idle state / mode cards

On the idle state (no results yet), show an engaging empty state. When the tool has distinct modes or non-obvious input formats, show 2–3 example usage cards that each have:
- A title describing the mode.
- A one-line description.
- A clickable example query that pre-fills the input.

Cards must use the `.mode-card` CSS class from `components.css`. For tools with a single obvious mode, a brief descriptive tagline and 2–3 clickable example domain/query chips is the minimum — never leave the idle state as a blank input box.

---

## 7. Input & Query UX

- Placeholder must show a real example (e.g. `example.com or example.com:443,8443`), not generic text.
- Input **must** have `aria-label` describing its purpose (e.g. `"Hostname to inspect"`).
- Include an `×` clear button inside the input when it has content (`type="button"`, `aria-label="Clear"`, `tabIndex={-1}`).
- Query history: full-width clickable buttons, deduplicated, capped at 20 entries, stored via `storageGet/storageSet`.
- If the tool has distinct common modes: include 2–3 quick-select preset chips (ghost/outline style) below the input.
- For combobox inputs with history dropdown: use `role="combobox"`, `aria-expanded`, `aria-autocomplete="list"`, `aria-controls` referencing the listbox id.

### Primary button loading state

The primary action button **must change its label** to a progressive form during loading (e.g. "Inspect" → "Inspecting…", "Look up" → "Looking up…", "Query" → "Querying…"). Never leave it static while a request is in flight. A spinner inside the button is optional but welcome alongside the label change.

---

## 8. Results & Error Display

- **Errors**: Inline in the results area. Red border + light red background box. `role="alert"`. Not a toast, not a modal.
- **Loading**: Status indicator with `role="status"` `aria-live="polite"`.
- **Validation summary**: At the top of results, show colored chips for pass/fail/warn/skip counts before the detail sections.
- **Cross-tool links**: Use `<CrossLink>` from `netray-common-frontend`. Target mappings: IPs → `ip_base_url`, domains → `dns_base_url`, TLS targets → `tls_base_url`. Always `encodeURIComponent()`.
- **Toasts**: Only for ephemeral user actions (copy, export). Duration: 2s. `role="status"` `aria-live="polite"`.

---

## 9. API Client (`lib/api.ts`)

```ts
function fetchWithTimeout(url: string, init: RequestInit = {}, timeoutMs = 5000): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  return fetch(url, { ...init, signal: controller.signal }).finally(() => clearTimeout(timer));
}

export async function fetchData(param: string): Promise<ResponseType> {
  const res = await fetchWithTimeout(`/api/endpoint?q=${encodeURIComponent(param)}`, {
    headers: { Accept: 'application/json' },
  });
  if (!res.ok) {
    const body = await res.json().catch(() => null);
    throw new Error(body?.error?.message ?? `HTTP ${res.status}`);
  }
  return res.json();
}

export async function fetchMeta(): Promise<MetaResponse | null> {
  try {
    const res = await fetch('/api/meta');
    if (!res.ok) return null;
    return res.json();
  } catch {
    return null;
  }
}
```

---

## 10. SolidJS Patterns

- No prop destructuring in function signatures — access via `props.field`.
- `export default` only — no named component exports.
- `<Show when={...}>` for conditionals, `<For each={...}>` for lists — no ternary JSX.
- Async data: `createSignal` + `onMount` + try/catch/finally. Do not use `createResource`.
- `ErrorBoundary` wraps `<App>` in `index.tsx`.
- Component-scoped styles via inline `<style>` tag inside the component (not a separate CSS file).

---

## 11. Styling

- **CSS custom properties only** — no utility classes, no Tailwind, no CSS-in-JS library.
- **Dark-mode by default**; `[data-theme="light"]` overrides on `:root`.
- Light mode must remap all color tokens. The accent and semantic colors change substantially:

  | Token | Dark | Light |
  |-------|------|-------|
  | `--accent` | `#00d4ff` | `#0077cc` |
  | `--accent-secondary` | `#7b68ee` | `#7b68ee` |
  | `--pass` | `#22c55e` | `#008800` |
  | `--fail` | `#ef4444` | `#cc0000` |
  | `--warn` | `#f59e0b` | `#b86e00` |
  | `--skip` | `#94a3b8` | `#4a5568` |

- `--accent-secondary` is the purple endpoint of the logo gradient; it does not change between themes.
- Define tool-specific semantic tokens in `:root` (e.g. `--pass`, `--fail`, `--warn`, `--skip`, record-type colors) — never write raw hex values in component CSS.
- Never override base tokens from `theme.css` without a documented reason.

### `.btn-primary` — the primary action button

All primary CTA buttons (submit, look up, query, inspect) must use the `.btn-primary` class from `components.css`. **Do not write per-tool button styles that duplicate this.** If a layout-only override is needed (e.g. `flex-shrink: 0` for inline positioning), add a second class alongside `.btn-primary`.

The canonical style is:
- `background: var(--accent)`, `color: #fff`, `border: none`
- `font-family: var(--font-mono)`, `font-size: 0.875rem`, `font-weight: 600`
- `min-height: 37px`, `padding: 0.5rem 1.25rem`, `border-radius: var(--radius)`
- Hover: `background: var(--accent-dim)`, `box-shadow: 0 0 12px var(--accent-glow)`
- Disabled: `opacity: 0.5`, `cursor: not-allowed`

---

## 12. Accessibility

| Element | Requirement |
|---------|-------------|
| Primary action button (submit) | Min 37px tall |
| Secondary buttons (toolbar) | Min 32×32px |
| Suite nav links | Min 44px touch target on mobile via padding |
| Icon-only buttons | `aria-label` required |
| Query input | `aria-label` required (not just placeholder) |
| Error messages | `role="alert"` |
| Loading indicators | `role="status"` `aria-live="polite"` |
| Toast notifications | `role="status"` `aria-live="polite"` |
| Skip link | Visually hidden, revealed on `:focus` |
| Help modal | `role="dialog"`, `aria-modal="true"`, `aria-labelledby` |

- Keyboard shortcuts must skip `INPUT`, `TEXTAREA`, `contenteditable`, `.cm-editor`.

---

## 13. History & Persistence

```ts
// lib/history.ts
const STORAGE_KEY = 'toolname_history';
const MAX_ENTRIES = 20;

export function addToHistory(query: string): void {
  const entries = getHistory().filter(e => e.query !== query);
  entries.unshift({ query, timestamp: Date.now() });
  if (entries.length > MAX_ENTRIES) entries.length = MAX_ENTRIES;
  storageSet(STORAGE_KEY, entries);
}
```

- Storage key: `toolname_history` (use the tool's short name).
- Deduplicate on insert: remove existing entry before unshift.
- Cap at 20 entries.

---

## 14. Testing

- **Utility tests** (`lib/*.ts`): Test all non-trivial utility functions — history, parsers, formatters, domain logic. Use `node` environment. No DOM, no mocking needed.
- **Component tests** (`components/*.tsx`): Test components with real interaction logic (multi-step state, keyboard nav, API calls). Use `happy-dom` + `@solidjs/testing-library`.
- **Do not test** pure display components — they're covered by visual inspection.
- **Mock `fetch`** globally in component tests via `vi.stubGlobal`.
- **Mock `localStorage`** via `vi.stubGlobal` in a `src/test-setup.ts` (used by `happy-dom` vitest config).
- **Test files**: Co-located with source — `lib/foo.test.ts` next to `lib/foo.ts`.
- **Naming**: `describe('functionName')` / `it('does X when Y')`.
