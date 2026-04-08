# CLAUDE.md — lens

## Rules

- Do NOT add a `Co-Authored-By` line for Claude in commit messages.
- **SCORING SYNC RULE**: Any change to the scoring algorithm (`src/scoring/`),
  profile schema, weight tiers, grade thresholds, or hard-fail rules MUST be
  reflected in `README.md` (the Scoring section) in the same commit. The README
  is the transparency contract — users must understand why a domain received a
  grade without reading source code.
- Don't add heavy dependencies for minor convenience.
- Don't mix formatting-only changes with functional changes in the same commit.
- Don't modify unrelated modules while you're in there.
- Don't add speculative flags, config options, or abstractions without a current caller.
- Don't bypass failing checks (`--no-verify`, `#[allow(...)]`) without explaining why.
- Don't include PII, real email addresses, or real domains (other than example.com) in test data, docs, or commits.
- If uncertain about an implementation detail, leave a concrete `TODO("reason")` rather than a hidden guess.

## Engineering Principles

- **KISS**: Simplest solution that works.
- **YAGNI**: Don't build for hypothetical future requirements.
- **Fail Fast**: Validate at boundaries, return errors early.
- **Secure by Default**: lens makes outbound HTTP calls to user-specified domains — target policy validation is load-bearing.
- **Partial failure is expected**: one backend down must never block the others.

## Project Overview

**lens** is the unified domain health check service for the netray.info suite
(`lens.netray.info`). It takes a domain, calls mhost-prism (DNS), tlsight (TLS),
and ifconfig-rs (IP) in parallel, and streams aggregated results via SSE with a
letter-graded health score.

- DNS + TLS run in parallel via `tokio::join!`
- IP enrichment runs after DNS (needs resolved IPs)
- Results stream as SSE events: `dns`, `tls`, `ip`, `summary`, `done`
- 20s hard deadline — return whatever is ready
- Scoring driven by external TOML profile (embedded default, file override)

## Architecture

```
lens/
  Cargo.toml
  Makefile
  Dockerfile
  lens.example.toml
  profiles/
    default.toml          # Default scoring profile (embedded at compile time)
  src/
    main.rs               # Entry point, Axum server, graceful shutdown
    config.rs             # TOML + LENS_ env vars
    error.rs              # AppError enum → HTTP status + error codes
    input.rs              # Domain validation (no IPs, no wildcards, max 253 chars)
    state.rs              # AppState (config, rate limiters, reqwest client, cache, profile)
    routes.rs             # Route definitions + handlers
    check.rs              # Orchestration: parallel DNS+TLS, then IP; SSE streaming
    backends/
      mod.rs
      dns.rs              # Call prism, parse CollectedResponse, extract lint + IPs
      tls.rs              # Call tlsight, parse quality checks
      ip.rs               # Call ifconfig-rs /batch, map network.type to verdict
    scoring/
      mod.rs
      profile.rs          # Profile, SectionProfile structs (serde from TOML)
      engine.rs           # compute_section, compute_overall, apply_hard_fails
    cache.rs              # moka TTL cache keyed by domain
    security/
      mod.rs
      rate_limit.rs       # GCRA per-IP + global
      ip_extract.rs       # Client IP from proxy headers
      target_policy.rs    # No RFC1918, no IPs, domain-only
  frontend/               # SolidJS + Vite (strict TypeScript)
```

## Scoring Algorithm

See README.md — Scoring section. That is the authoritative description.
Any code change to scoring must update README.md in the same commit.

## Key Dependencies

- `netray-common` 0.5 — IP extraction, security headers, GCRA wrappers, request IDs
- `axum` 0.8 — Web framework
- `reqwest` 0.12 — Backend HTTP calls (rustls-tls, no OpenSSL)
- `moka` 0.12 — In-process TTL cache
- `tokio-stream` + `futures` — SSE streaming
- `config` — TOML + env var layering (LENS_ prefix, __ separator)
- `toml` — Scoring profile deserialization

## Frontend Rules

Full spec: [`specs/frontend-rules.md`](../specs/frontend-rules.md) in the netray.info meta repo.

### Directory & Tooling
- Mirror structure from tlsight: `src/{index.tsx,App.tsx,components/,lib/,styles/global.css}` + `vite.config.ts`, `vitest.config.ts`, `tsconfig.json`, `package.json`, `.npmrc`
- No barrel `index.ts` files — import directly
- tsconfig: `strict: true`, `jsx: "preserve"`, `jsxImportSource: "solid-js"`, `moduleResolution: "bundler"`
- Build: `tsc && vite build`; dev proxy: `/api` → `http://127.0.0.1:808x` (next port after 8081)
- Separate `vitest.config.ts`: `happy-dom` for component tests, `node` for utility tests
- CI `npm ci` steps must set `NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}`

### Common-Frontend — Mandatory
- Import all four shared stylesheets in `global.css`: `theme.css`, `reset.css`, `layout.css`, `components.css`
- Theme: `createTheme('toolname_theme', 'system')` + `<ThemeToggle>` — never custom
- Footer: `<SiteFooter>` with aboutText, links (GitHub, `/docs`, Author), version from `/api/meta`
- Modals: always `<Modal>` (includes focus trap); localStorage: always `storageGet/storageSet`
- Keyboard shortcuts: `createKeyboardShortcuts()` — handles editor exclusions automatically

### Suite Navigation
- Use `<SuiteNav>` from `netray-common-frontend` (BEM: `suite-nav`, `suite-nav__brand`, `suite-nav__sep`, `suite-nav__link`, `suite-nav__link--active`)
- Labels uppercase: IP, DNS, TLS, LENS. Current tool: `suite-nav__link--active` + `aria-current="page"`
- All URLs from `meta.ecosystem.*_base_url` — no hardcoded production URLs. Fall back to `https://*.netray.info`

### Meta Endpoint
- Fetch `/api/meta` on mount; set `document.title` from `meta.site_name`; failure must never block the tool
- Cross-tool deep links: always `meta().ecosystem.*_base_url` + `encodeURIComponent()`

### Page Structure
- `<h1>` = tool name; tagline as adjacent `<span>` — not in the h1
- Required landmarks: `<nav>`, `<main>`, `<footer>`
- Skip link ("Skip to results"), visually hidden, revealed on `:focus`
- `?` help button (min 32×32px) in toolbar → `<Modal>`
- Example usage cards on idle state when tool has distinct modes or non-obvious inputs

### Input UX
- Placeholder: real example, not generic text
- Input must have `aria-label` (not just placeholder)
- `×` clear button inside input when non-empty (`type="button"`, `aria-label="Clear"`, `tabIndex={-1}`)
- Combobox with history: `role="combobox"`, `aria-expanded`, `aria-autocomplete="list"`, `aria-controls`
- History: max 20 entries, deduplicated on insert, stored as `toolname_history` via `storageSet`
- Preset chips (if applicable): ghost/outline style below input

### Results & Errors
- Errors: inline red-border box in results area, `role="alert"` — not toast, not modal
- Validation summary: pass/fail/warn/skip chip row at top of results
- Loading: `role="status"` `aria-live="polite"`
- Toasts: ephemeral actions only (copy, export), 2s, `role="status"` `aria-live="polite"`

### API Client
- All fetches via `fetchWithTimeout(url, init, timeoutMs=5000)`
- Extract backend error: `body?.error?.message ?? \`HTTP ${res.status}\``
- `fetchMeta()` returns `null` on failure — never throws

### SolidJS Patterns
- No prop destructuring; access via `props.field`
- `export default` only — no named component exports
- `<Show>` for conditionals, `<For>` for lists — no ternary JSX
- Async data: `createSignal` + `onMount` + try/catch/finally — not `createResource`
- `ErrorBoundary` wraps `<App>` in `index.tsx`
- Component-scoped styles: inline `<style>` tag inside the component

### Styling
- CSS custom properties only — no Tailwind, no utility classes, no CSS-in-JS
- Dark-mode default; `[data-theme="light"]` on `:root`. Light mode must remap ALL color tokens:
  `--accent: #0077cc`, `--pass: #008800`, `--fail: #cc0000`, `--warn: #b86e00`, `--skip: #4a5568`
- Tool-specific semantic tokens in `:root` (e.g. `--pass`, `--fail`) — never raw hex in component CSS

### Accessibility
- Primary buttons: min 37px tall; secondary/toolbar: min 32×32px; nav links: 44px touch target on mobile
- Icon-only buttons: `aria-label` required; query input: `aria-label` required (not just placeholder)
- Keyboard shortcuts skip `INPUT`, `TEXTAREA`, `contenteditable`, `.cm-editor`

### Testing
- Test all non-trivial `lib/` utilities: history, parsers, formatters, domain logic (`node` environment)
- Test components with real interaction logic: `happy-dom` + `@solidjs/testing-library`
- Mock `fetch` via `vi.stubGlobal`; mock `localStorage` in `src/test-setup.ts`
- Test files co-located: `lib/foo.test.ts` next to `lib/foo.ts`

## Logging & Telemetry

Rules: [`specs/logging-rules.md`](../specs/logging-rules.md) in the netray.info meta repo. Follow those rules when modifying tracing init, log filters, or `[telemetry]` config.

Default filter: `info,lens=debug,hyper=warn,h2=warn`. Telemetry config via `[telemetry]` section or `LENS_TELEMETRY__*` env vars. Production uses `log_format = "json"` and `service_name = "lens"`.

## CI/CD

Workflow rules: [`specs/workflow-rules.md`](../specs/workflow-rules.md) in the netray.info meta repo. Follow those rules when creating or modifying any `.github/workflows/*.yml` file.

Workflows: `ci.yml` (PR gate: fmt, clippy, test, frontend, audit), `release.yml` (tag-push: test → build → merge), `deploy.yml` (fires after release via webhook).

## Build & Test

```sh
make          # frontend + release binary
make dev      # cargo run (dev mode)
make test     # Rust + frontend tests
make ci       # lint + test + frontend
```

Live reference domain tests are gated behind `#[ignore]` and `LENS_LIVE_TESTS=1`.
