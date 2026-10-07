# @netray-info/common-frontend

Shared SolidJS utilities, primitives, and CSS design tokens for the [netray.info](https://netray.info) service family (ifconfig-rs, mhost-prism, tlsight).

## Install

```sh
npm install @netray-info/common-frontend
```

The package is published to the GitHub Packages registry:

```sh
# .npmrc
@netray-info:registry=https://npm.pkg.github.com
```

## Exports

### `@netray-info/common-frontend/theme`

SolidJS primitive for dark/light/system theme management.

```ts
import { createTheme } from '@netray-info/common-frontend/theme';

const { theme, resolved, setTheme, toggleTheme } = createTheme();
// theme()    — 'dark' | 'light' | 'system' (persisted to localStorage)
// resolved() — 'dark' | 'light' (actual rendered value)
// setTheme(t) — set and persist a specific theme
// toggleTheme() — cycle dark → light → system → dark
```

`createTheme` reads from `localStorage`, falls back to `prefers-color-scheme`, applies a `data-theme` attribute to `document.documentElement`, and listens for system theme changes. The `storageKey` and `defaultTheme` parameters are optional.

### `@netray-info/common-frontend/keyboard`

Register global keyboard shortcuts that are suppressed when focus is inside an input, textarea, contenteditable element, or CodeMirror editor.

```ts
import { createKeyboardShortcuts } from '@netray-info/common-frontend/keyboard';

const cleanup = createKeyboardShortcuts({ '/': (e) => focusSearch() });
onCleanup(cleanup);
```

Returns a cleanup function suitable for SolidJS `onCleanup`.

### `@netray-info/common-frontend/focus-trap`

Creates a focus trap within a container element. Traps Tab/Shift+Tab within the container, calls `onEscape` on Escape, and restores focus on deactivate.

```ts
import { createFocusTrap } from '@netray-info/common-frontend/focus-trap';

const trap = createFocusTrap(() => dialogRef, () => closeDialog());
trap.activate();   // focus first focusable element inside container
trap.deactivate(); // restore focus to element active before activation
```

### `@netray-info/common-frontend/styles/theme.css`

CSS custom properties (design tokens) shared across all services. Dark-mode by default; light mode under `[data-theme="light"]`.

```ts
import '@netray-info/common-frontend/styles/theme.css';
```

Tokens include `--bg-primary`, `--bg-secondary`, `--bg-tertiary`, `--text-primary`, `--text-secondary`, `--text-muted`, `--accent`, `--success`, `--warning`, `--error`, `--border`, `--font-mono`, `--font-sans`, `--radius`, and `--transition`. Import this once in the entry point of each service to share the design language.

## TypeScript

The source is published as-is (TypeScript, not transpiled). Consuming projects must use Vite (or another bundler with TypeScript support) to compile it. No build step is needed in this package.

`solid-js` is a peer dependency and must be installed in the consuming project.

## License

MIT
