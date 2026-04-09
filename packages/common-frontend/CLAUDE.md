# CLAUDE.md — netray-common-frontend

## Rules

- No Co-Authored-By for Claude in commits
- Scoped changes only: no formatting mixed with functional changes, no unrelated modifications
- No heavy deps for minor convenience; no speculative flags/config/abstractions without a caller
- Don't bypass failing checks (`--no-verify`, `#[allow(...)]`) without explaining why
- No PII, real emails, or real domains (use example.com) in test data, docs, commits
- `TODO("reason")` over hidden guesses; conventional commits (`feat:`, `fix:`, `refactor:`, etc.)

## Frontend Rules

Full spec: [`specs/rules/frontend-rules.md`](../specs/rules/frontend-rules.md) in the netray.info meta repo. Apply when modifying components, CSS tokens, or the theme system.

## Project Overview

**netray-common-frontend** is a shared SolidJS package (`@netray-info/common-frontend`) published to GitHub Packages. It provides the theme system, shared UI components, keyboard utilities, and CSS design tokens consumed by all tool frontends.

- **Registry**: GitHub Packages (`https://npm.pkg.github.com`, scope `@netray-info`)
- **Consumers**: ifconfig-rs, mhost-prism, tlsight, lens frontends

## CI/CD

Workflow rules: [`specs/rules/workflow-rules.md`](../specs/rules/workflow-rules.md) in the netray.info meta repo. Follow those rules when creating or modifying any `.github/workflows/*.yml` file.

Workflows: `ci.yml` (PR gate: lint, test, audit), `publish.yml` (tag-push: test → publish to GitHub Packages).

Publishing: tag push matching `v*.*.*` triggers `publish.yml`. Manual `workflow_dispatch` also supported. `NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` is required on every `npm ci` and `npm publish` step.

## Build & Test

```sh
npm ci                  # install deps (requires NODE_AUTH_TOKEN if private deps)
npm run lint            # ESLint / type check
npm test                # vitest
npm run build           # compile to dist/
```
