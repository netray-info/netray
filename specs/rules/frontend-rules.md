# Frontend Rules for New netray.info Tools

Derived from analysis of all six suite tools (ifconfig-rs, mhost-prism, tlsight, spectra, beacon, lens) and netray-common-frontend.
Validated against live UI via Playwright scan and manual cross-tool review (last updated 2026-04-10).

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
<SuiteNav current="dns" meta={meta()?.ecosystem} />
```

- Labels are **uppercase short names**: IP, DNS, TLS, LENS. Current tool gets `suite-nav__link--active` + `aria-current="page"`.
- BEM classes: `suite-nav`, `suite-nav__brand`, `suite-nav__sep`, `suite-nav__link`, `suite-nav__link--active`.
- All URLs come from `meta.ecosystem.*_base_url` — no hardcoded production URLs in source. Fall back to `https://*.netray.info` if meta is unavailable.
- **Do not redefine `.suite-nav*` CSS** in tool-level `global.css`. The `<SuiteNav>` component owns its styles via an inline `<style>` tag. Any local redefinition will conflict.

### SuiteNav visual requirements

- **Inside the content container**: Place `<SuiteNav>` inside the `.app` wrapper so it aligns with the content width. The lens-style card background is the reference — a subtle `--bg-secondary` background with rounded corners.
- **Compact**: The nav should be visually lightweight — small text, tight padding. It provides context, not a primary interaction surface. It should not dominate vertical space.
- **Consistent across all tools**: Every tool (except ifconfig-rs, see §6.1) must render `<SuiteNav>` identically. No per-tool custom styling around the nav.

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

- `document.title` set from `meta.site_name` on mount; fall back to `location.hostname`.
- Required landmarks: `<nav>` (suite nav), `<main>` (input + results), `<footer>` (SiteFooter).
- Include a visually-hidden "Skip to results" skip link (revealed on focus).

### 6.1. Canonical page layout

The canonical page structure for DNS, TLS, and Lens tools is:

```
┌─────────────────────────────────────────────┐
│  SuiteNav (full viewport width, compact)    │
├─────────────────────────────────────────────┤
│                                             │
│  ┌─ header ──────────────────────────────┐  │
│  │  <h1 class="logo">toolname</h1>      │  │
│  │  <span class="tagline">…</span>      │  │
│  │                    ThemeToggle  ?Help  │  │
│  └───────────────────────────────────────┘  │
│  ─────────── horizontal separator ────────  │
│                                             │
│  [input area]                               │
│  [results area]                             │
│                                             │
│  <SiteFooter />                             │
└─────────────────────────────────────────────┘
```

**Header row** (the `<header class="header">` element):
- **Left side**: `<h1 class="logo">` with tool name (gradient text via `--accent` → `--accent-secondary`), followed by `<span class="tagline">` with a short phrase.
- **Right side**: `<div class="header-actions">` containing `<ThemeToggle>` then `<button class="header-btn">?</button>` (help). Order: theme toggle first, help button second.

**Tagline convention** — taglines follow the `"[noun], [past-tense verb]"` format and favour optical or revealing metaphors that match the suite's identity:

| Tool | Tagline |
|------|---------|
| ifconfig-rs | IP, decoded |
| mhost-prism | DNS, refracted |
| tlsight | TLS, illuminated |
| spectra | HTTP, exposed |
| beacon | email security, graded |
| lens | domains, in focus |

New tools must pick a tagline in this style — punchy, snarky, and under five words. Avoid generic descriptive phrases like "Domain health at a glance" or "Email security inspector".
- A visible **horizontal separator** (`<hr>` or border-bottom) below the header, before the input area. This visually anchors the header and separates it from the content.
- **Spacing**: There should be clear vertical breathing room between the SuiteNav bar and the header row (roughly `1rem`–`1.5rem`).

**Footer**: Every tool must use `<SiteFooter>` from `netray-common-frontend`. No exceptions.

**ifconfig-rs exception**: The IP tool has a unique dashboard-style layout (floating header-actions, site-header with share button, card grid). It does not follow the canonical header row pattern. Do not attempt to unify it — keep its current layout.

### 6.2. Elements that do NOT belong in the layout

- **Instructional callouts** between the input and results (e.g. "Results stream as they arrive"). Remove these — the UI should be self-evident.
- **Redundant descriptions** that restate the tagline or tool purpose below the header.

### 6.3. Idle state / mode cards

On the idle state (no results yet), show an engaging empty state. When the tool has distinct modes or non-obvious input formats, show 2–3 example usage cards that each have:
- A title describing the mode.
- A one-line description.
- A clickable example query that pre-fills the input.

Cards must use the `.mode-card` CSS class from `components.css`. For tools with a single obvious mode, a brief descriptive tagline and 2–3 clickable example domain/query chips is the minimum — never leave the idle state as a blank input box.

**`netray.info` is always an example** — at least one example query in every tool (except ifconfig-rs, which shows your own IP) must use `netray.info` as the target. This dogfoods the suite and demonstrates real-world output to new users. Subsequent examples should use `example.com` (for documentation-safe demos) and one real well-known domain (e.g. `github.com`, `gmail.com`).

---

## 7. Input & Query UX

- Placeholder must show a real example (e.g. `example.com or example.com:443,8443`), not generic text.
- Input **must** have `aria-label` describing its purpose (e.g. `"Hostname to inspect"`).
- Include an `×` clear button inside the input when it has content (`type="button"`, `aria-label="Clear"`, `tabIndex={-1}`).
- Query history: full-width clickable buttons, deduplicated, capped at 20 entries, stored via `storageGet/storageSet`.
- If the tool has distinct common modes: include 2–3 quick-select preset chips (ghost/outline style) below the input.
- For combobox inputs with history dropdown: use `role="combobox"`, `aria-expanded`, `aria-autocomplete="list"`, `aria-controls` referencing the listbox id.

### Copy-link (share) button

Every tool that encodes its query in the URL **must** include a copy-link button:

- **Position**: standalone button in the input row, between the field-wrap and the primary action button.
- **Appearance**: use the `.share-btn` class — bordered, accent-colored, icon-only (chain-link SVG). Same height as the primary button (`align-self: stretch`).
- **Visibility**: render only when a result is present (i.e. after the first successful inspection). Do not show on the empty/idle state.
- **Behavior**: copies `window.location.href` (the URL already contains the query param after inspection). On success, swap the icon for a checkmark SVG for 2 s, then revert.
- **Accessibility**: `type="button"`, `aria-label="Copy shareable link"`, `title` shows "Copy shareable link" or "Copied!" dynamically.
- Implement inside the input component (e.g. `UrlInput.tsx`) via a `showCopyLink?: boolean` prop passed from the parent. The parent sets `showCopyLink={!!result()}`.

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

## 8a. Result Overview Box

The result overview box is the summary card that appears directly below the input after a successful inspection. It is always visible while results are shown and provides the at-a-glance answer before the user reads any detail section.

### Structure

Two rows inside a bordered card (`div.overview`):

**Row 1 — Response facts** (left-aligned data items + export actions pushed right):

| Item | Label | Value notes |
|------|-------|-------------|
| Verdict | `Verdict` | Colored badge reflecting the worst check status |
| HTTP status | `Status` | Numeric code (e.g. `200`) |
| Protocol version | `HTTP` | e.g. `h2`, `h3`, `http/1.1` |
| Response time | `Duration` | Color-coded: green < 500 ms, amber < 2 s, red ≥ 2 s |
| Export actions | — | Right-aligned text links (see below) |

**Row 2 — Server / hosting context** (separated by a top border):

| Item | Label | Value notes |
|------|-------|-------------|
| Server IP | `Server` | The IP that served the response |
| Hosting provider | `Hosted by` | ASN org name + infra type in muted parens, e.g. `Oracle Corporation (Cloud)` |

Show row 2 items conditionally — omit `Hosted by` when enrichment data is unavailable.

### Item format

Each item follows the `overview__item` pattern:

```tsx
<div class="overview__item">
  <span class="overview__label">Label</span>
  <span class="overview__value">value</span>
</div>
```

- Labels: `var(--text-muted)`, monospace, `cursor: default`.
- Values: `font-weight: 600`, monospace.
- Qualifier text within a value (e.g. the infra type in parentheses): `overview__value--qualifier` — normal weight, muted color.

### Export actions

Placed as the last element of row 1, pushed to the far right via `margin-left: auto` on the `.export-buttons` container.

- Style: **borderless text links** in `var(--text-muted)`, monospace, `font-size: 0.75rem`. Hover → `var(--accent)`.
- Separated by a thin `border-left: 1px solid var(--border)` between buttons — no gaps, no backgrounds.
- Current actions: **copy MD** (copies a Markdown summary to clipboard) and **JSON** (downloads the full result as a `.json` file).
- Copy confirmation: label changes to `copied!` for 2 s on success, then reverts — no toast.
- Do not place export actions outside the overview box (not in a separate controls row, not below the sections).

### Section controls row

Directly below the overview box, a single `section-controls` row provides section-level navigation:

- **Left**: `explain` toggle — activates inline explanation text inside each section.
- **Right**: `Expand all` / `Collapse all` toggle.

No other controls belong in this row.

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

---

## 15. Verdict Badges

All tools that display pass/fail/warn/skip verdicts must use a single standardized badge style.

### Shape and layout

- **Pill-shaped** badges: `border-radius: 9999px`, inline, compact (`padding: 0.125rem 0.5rem`).
- **Icon + label**: A small icon (checkmark, X, warning triangle, dash) followed by the verdict text in uppercase or title case.
- Badges may appear standalone (in section headers) or with a count suffix (e.g. `24 pass`).

### Colors

Use the semantic color tokens — never raw hex in badge CSS:

| Verdict | Token | Icon |
|---------|-------|------|
| Pass | `var(--pass)` | Checkmark |
| Fail | `var(--fail)` | X / cross |
| Warn | `var(--warn)` | Warning triangle or `!` |
| Skip | `var(--skip)` | Dash `—` |

### Variants

- **Filled** (default): Colored background, white text. Use in summary chips and section headers.
- **Outlined**: Transparent background, colored border + text. Use for inline annotations within check rows.

### Where they apply

- **Summary bar** (below input, above results): Show aggregated counts as filled badges — e.g. `[v 24 pass] [! 4 warn] [x 5 fail] [— 1 skip]`.
- **Section headers**: A single filled badge showing the worst verdict for that section.
- **Individual check rows**: An outlined or filled badge at the start of the row.

**Do not** invent per-tool badge styles. If `components.css` in `netray-common-frontend` does not yet provide a `.verdict-badge` class, add it there — not in tool CSS.

---

## 16. Collapsible Result Sections

Tools that display results in multiple categories (TLS checks, DNS checks, security headers, etc.) must use collapsible card sections.

### When to use

- Any results view with **3+ distinct categories** of output should group them into collapsible sections.
- **DNS/prism** is exempt — its table-row expand pattern is appropriate for record-by-record results.
- **IP/ifconfig-rs** is exempt — its tab + card grid layout is an intentional exception.

### Behavior

- **Collapsed by default**: All sections start collapsed. The user sees the summary and expands what they care about.
- **Expand all / collapse all**: A toggle button in the top-right of the results area, above the section list. Label: `expand all` / `collapse all`.
- **Persistent state**: Do not persist expand/collapse state across page loads. Always start collapsed.
- **Animated**: Use a brief CSS transition on `max-height` or `grid-template-rows` — no jarring jump.

### Card anatomy

```
┌─────────────────────────────────────────────────┐
│ [status] Title [summary badges]  [deep-link] [v]│
├─────────────────────────────────────────────────┤
│ (expanded content — check rows, details, etc.)  │
└─────────────────────────────────────────────────┘
```

- Light border (`var(--border)`), `border-radius: var(--radius)`.
- No heavy drop shadows in dark mode. A `1px` border is sufficient.
- Spacing between cards: `0.75rem`.

---

## 17. Section Header Anatomy

Every collapsible result section must have a consistent header row:

```
[status dot] Title [inline summary badges] ............. [deep-link icon] [chevron]
```

| Element | Required | Description |
|---------|----------|-------------|
| **Status dot** | Yes | Colored circle (`8px`) using `--pass`/`--warn`/`--fail` for the worst verdict in the section |
| **Title** | Yes | Category name in `font-weight: 600` (e.g. "TLS", "DNS", "Security Headers") |
| **Summary badges** | Yes | Inline verdict badges or short check results (e.g. `x Chain of Trust`, `v HSTS`) |
| **Deep-link icon** | Conditional | Only when the section maps to another netray tool (see §19). Uses `<CrossLink>` |
| **Chevron** | Yes | `v` when collapsed, `^` when expanded. Rotates via CSS transform |

- The header row is the click target for expand/collapse — the entire row is clickable, not just the chevron.
- `cursor: pointer` on the header.
- `aria-expanded="true|false"` on the header element.

---

## 18. Check Row Anatomy

Individual check results within an expanded section follow this layout:

```
[verdict badge]  Check name                    Value / detail
```

| Element | Description |
|---------|-------------|
| **Verdict badge** | Filled or outlined pill (§15) — PASS, FAIL, WARN, or SKIP |
| **Check name** | `font-weight: 500`, left-aligned. Human-readable name (e.g. "HSTS", "Chain of Trust", "SPF Record") |
| **Value / detail** | `font-family: var(--font-mono)`, right-aligned or after a flexible spacer. The raw value, header content, or explanation |

### Row background tinting

- **Failed** checks: Subtle red tint — `background: color-mix(in srgb, var(--fail) 8%, transparent)`.
- **Warned** checks: Subtle orange tint — `background: color-mix(in srgb, var(--warn) 6%, transparent)`.
- **Passed / skipped**: No background tint — default transparent.

This tinting provides a scannable visual heat map without being overwhelming.

### Supplementary detail

If a check has additional explanation (e.g. a full CSP directive breakdown, a certificate chain), show it as an indented sub-block below the check row. Use `padding-left: 2rem` and a slightly dimmer text color (`var(--text-secondary)`).

---

## 19. Cross-Tool Deep Links in Section Headers

When a result section maps directly to another netray tool, the section header should include a subtle deep-link icon/button that opens the relevant tool for the same target.

### Mapping

| Section | Links to | URL pattern |
|---------|----------|-------------|
| TLS (in lens) | tlsight | `{tls_base_url}/?host={domain}` |
| DNS (in lens) | prism | `{dns_base_url}/?q={domain}` |
| IP (in lens) | ifconfig-rs | `{ip_base_url}/?ip={ip}` |
| Security Headers (in spectra) | — | No cross-link (spectra is the authority) |

- Use `<CrossLink>` from `netray-common-frontend`.
- The link appears as a small external-link icon or the tool's short name (e.g. `TLS >`) at the right side of the section header, before the chevron.
- `title` attribute: "Open in {tool name}" (e.g. "Open in tlsight").
- Only show when `meta.ecosystem.*_base_url` is available. Hide gracefully if meta fetch failed.

---

## 20. Help Modal Structure

Every tool must have a `?` help button in the header (theme toggle first, then `?`) that opens a `<Modal>` from `netray-common-frontend`. The modal title is always `"Help"`.

### Required section order

```
1. About          — one-paragraph description of what the tool does + link to netray.info guide
2. [Reference]    — tool-specific content (query syntax, flags, record types, scoring, etc.)
3. Keyboard shortcuts — table listing all keyboard shortcuts
```

The "About" section is always first. The "Keyboard shortcuts" section is always last. Tool-specific reference sections go in the middle.

### About section

```tsx
<div class="help-section">
  <div class="help-section__title">About</div>
  <p class="help-desc">
    toolname does X, Y, and Z.{' '}
    <a href="https://netray.info/guide/" target="_blank" rel="noopener noreferrer">
      Reference guides ↗
    </a>
  </p>
</div>
```

- One concise paragraph. Not a marketing blurb — describe what the tool actually does.
- Always link to `https://netray.info/guide/` (or a specific guide page if one is particularly relevant).

### Keyboard shortcuts section

Use the `shortcuts-table` CSS pattern — **not** the `help-keys` / `help-key` div-based layout:

```tsx
<div class="help-section">
  <div class="help-section__title">Keyboard shortcuts</div>
  <table class="shortcuts-table">
    <thead>
      <tr><th>Key</th><th>Action</th></tr>
    </thead>
    <tbody>
      <tr><td class="shortcut-key">/</td><td>Focus input</td></tr>
      <tr><td class="shortcut-key">Enter</td><td>Submit (when input focused)</td></tr>
      <tr><td class="shortcut-key">r</td><td>Re-run last query</td></tr>
      <tr><td class="shortcut-key">j / k</td><td>Navigate result cards</td></tr>
      <tr><td class="shortcut-key">Enter</td><td>Expand / collapse active card</td></tr>
      <tr><td class="shortcut-key">Escape</td><td>Blur input / close help</td></tr>
      <tr><td class="shortcut-key">?</td><td>Toggle this help</td></tr>
    </tbody>
  </table>
</div>
```

Required CSS (add to tool `global.css` if not present):

```css
.shortcuts-table { width: 100%; border-collapse: collapse; }
.shortcuts-table th { padding: 4px 4px 8px; font-size: 0.6875rem; font-weight: 600;
  text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted);
  border-bottom: 1px solid var(--border); text-align: left; }
.shortcuts-table td { padding: 6px 4px; font-size: 0.875rem; color: var(--text-secondary);
  border-bottom: 1px solid var(--border); }
.shortcuts-table tr:last-child td { border-bottom: none; }
.shortcut-key { font-family: var(--mono); font-size: 0.8125rem; font-weight: 600;
  color: var(--accent); white-space: nowrap; width: 80px; }
```

---

## 21. Keyboard Navigation Pattern

Every query tool must implement keyboard navigation. The minimum required shortcuts are:

| Key | Action |
|-----|--------|
| `/` | Focus query input |
| `Enter` | Submit (when input focused) |
| `r` | Re-run last query |
| `j` / `k` | Navigate result cards (if tool has collapsible cards) |
| `Enter` | Expand / collapse active card (when card is focused) |
| `Escape` | Blur input / close help / deselect active card |
| `?` | Toggle help modal |

Use `createKeyboardShortcuts()` from `@netray-info/common-frontend/keyboard` for all shortcuts except `Escape` (which may need to work inside inputs).

### Card navigation implementation

For tools with collapsible result cards, use the `data-card` / `data-card-active` DOM attribute pattern — the same pattern across all suite tools:

```ts
// Add to onMount
function clearCardActive() {
  document.querySelector('[data-card-active]')?.removeAttribute('data-card-active');
}

function navigateCards(e: KeyboardEvent) {
  const cards = Array.from(document.querySelectorAll<HTMLElement>('[data-card]'));
  if (cards.length === 0) return;
  e.preventDefault();
  const cur = document.querySelector<HTMLElement>('[data-card-active]');
  let idx = cur ? cards.indexOf(cur) : -1;
  if (idx === -1) {
    idx = e.key === 'j' ? 0 : cards.length - 1;
  } else {
    cur!.removeAttribute('data-card-active');
    idx += e.key === 'j' ? 1 : -1;
  }
  idx = Math.max(0, Math.min(idx, cards.length - 1));
  cards[idx].setAttribute('data-card-active', '');
  cards[idx].scrollIntoView({ block: 'nearest', behavior: 'smooth' });
}

function expandActiveCard(e: KeyboardEvent) {
  const active = document.querySelector<HTMLElement>('[data-card-active]');
  if (active) {
    e.preventDefault();
    active.querySelector<HTMLElement>('.section-card__header')?.click();
  }
}

document.addEventListener('mousedown', clearCardActive);

const cleanupShortcuts = createKeyboardShortcuts({
  'j': navigateCards, 'k': navigateCards, 'Enter': expandActiveCard,
  // ... other shortcuts
});

onCleanup(() => {
  cleanupShortcuts();
  document.removeEventListener('mousedown', clearCardActive);
});
```

Add `data-card` to each top-level result card element. If the card is a component whose root you can't modify directly, wrap it: `<div data-card><MySection .../></div>`.

Required CSS (active card outline):

```css
[data-card-active] {
  outline: 2px solid var(--accent);
  outline-offset: -1px;
  border-radius: var(--radius, 4px);
}
```

The toggle button inside each card must use `.section-card__header` class so `expandActiveCard` can find it universally.

---

## 22. Compliance Matrix (2026-04-10)

What each tool must change to comply with the tightened rules above. ifconfig-rs is exempt from §6.1 (canonical layout) and §16 (collapsible sections). DNS/prism is exempt from §16–§18 (uses table-row expand pattern).

### mhost-prism (DNS)

| Rule | Status | Action |
|------|--------|--------|
| §4 SuiteNav inside .app | Non-compliant | Move `<SuiteNav>` inside `.app` container with card background |
| §6.1 Header row | Compliant | Already has logo + tagline left, theme + help right |
| §6.1 Horizontal separator | Non-compliant | Add visible border-bottom or `<hr>` below header |
| §6.1 SiteFooter | Compliant | Already uses `<SiteFooter>` |
| §6.2 Stream callout | Non-compliant | Remove `<p class="stream-hint">Results stream as they arrive…</p>` |
| §6.1 Header-actions order | Compliant | ThemeToggle then help button |
| §15 Verdict badges | N/A | DNS uses record-type badges, not verdict badges |
| §16 Collapsible sections | Exempt | Table-row expand pattern is appropriate |

### tlsight (TLS)

| Rule | Status | Action |
|------|--------|--------|
| §4 SuiteNav inside .app | Compliant | Already inside `.app` — verify card background styling |
| §6.1 Header row | Compliant | Already has logo + tagline left, theme + help right |
| §6.1 Horizontal separator | Check | Verify `<hr>` or border-bottom exists below header |
| §6.1 SiteFooter | Compliant | Already uses `<SiteFooter>` |
| §6.1 Header-actions order | Compliant | ThemeToggle then help button |
| §15 Verdict badges | Non-compliant | Has colored pills but not standardized shape/tokens — align to common `.verdict-badge` |
| §16 Collapsible sections | Non-compliant | Sections (Validation, CAA, TLS Params, Chain) are not collapsible — wrap in collapsible cards |
| §17 Section headers | Non-compliant | No status dot, no chevron, no consistent anatomy — adopt standard header pattern |
| §18 Check rows | Partial | Has badge + name but layout varies — standardize row anatomy |
| §19 Cross-tool deep links | Missing | Add deep links to prism (DNS) and ifconfig-rs (IP) in relevant sections |

### lens

| Rule | Status | Action |
|------|--------|--------|
| §4 SuiteNav inside .app | Compliant | Already inside `.app` |
| §6.1 Header row | Compliant | Already has logo + tagline left, help + theme right |
| §6.1 Horizontal separator | Check | Verify `<hr>` or border-bottom exists below header |
| §6.1 SiteFooter | Compliant | Already uses `<SiteFooter>` |
| §6.1 Header-actions order | Non-compliant | Swap order: ThemeToggle first, then help button |
| §15 Verdict badges | Partial | Has colored count chips — align shape/icons to common `.verdict-badge` |
| §16 Collapsible sections | Compliant | Already collapses by default with expand all toggle |
| §17 Section headers | Compliant | Already has status dot + title + badges + deep-link + chevron |
| §18 Check rows | Partial | Has verdict + name + value but tinting and layout vary — standardize |
| §19 Cross-tool deep links | Compliant | Already links to TLS, DNS, IP tools from section headers |

### spectra (HTTP)

| Rule | Status | Action |
|------|--------|--------|
| §4 SuiteNav inside .app | Check | Verify SuiteNav is inside `.app` with card background |
| §6.1 Header row | Partial | Has logo + tagline but missing help button; has theme toggle but different icon |
| §6.1 Horizontal separator | Compliant | Has separator below header |
| §6.1 SiteFooter | Missing | Add `<SiteFooter>` component |
| §6.1 Header-actions order | Non-compliant | Missing help button; add ThemeToggle then help button |
| §7 Copy-link button | Compliant | `.share-btn` between field-wrap and Inspect button; shown only when result present |
| §8a Result overview box | Compliant | Two-row overview with Server/Hosted-by row; export actions as text links in row 1 right |
| §11 Dark mode default | Non-compliant | Renders in light mode — switch default to dark |
| §15 Verdict badges | Non-compliant | Uses bold uppercase text labels, not pill badges — adopt `.verdict-badge` |
| §16 Collapsible sections | Non-compliant | Sections are flat/non-collapsible — wrap in collapsible cards, collapse by default |
| §17 Section headers | Non-compliant | Plain uppercase text titles — adopt standard header with status dot + badges + chevron |
| §18 Check rows | Non-compliant | Badge shape and row layout differ from standard — adopt check row anatomy |
| §19 Cross-tool deep links | Missing | Add deep link to ifconfig-rs (IP) where IP data is shown |

### ifconfig-rs (IP) — exception

| Rule | Status | Notes |
|------|--------|-------|
| §4 SuiteNav | Compliant | Uses `<SuiteNav>` |
| §6.1 Canonical layout | Exempt | Dashboard layout is an intentional exception |
| §6.1 SiteFooter | Compliant | Already uses `<SiteFooter>` |
| §15–§19 | Exempt | Card grid + tab layout is intentional |
