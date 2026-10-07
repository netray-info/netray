# CLAUDE.md — netray-common-frontend

## Frontend Rules

Full spec: [`specs/rules/frontend-rules.md`](../../specs/rules/frontend-rules.md). Apply when modifying components, CSS tokens, or the theme system.

## Project Overview

**netray-common-frontend** is a shared SolidJS package (`@netray-info/common-frontend`), a member of the root npm workspace. It provides the theme system, shared UI components, keyboard utilities, and CSS design tokens consumed by all tool frontends.

- **Distribution**: workspace member only, not published; the frontends under `crates/*/frontend` resolve it through the root `package.json` workspaces
- **Consumers**: every service frontend under `crates/*/frontend`

## CI/CD

Workflow rules: [`specs/rules/workflow-rules.md`](../../specs/rules/workflow-rules.md). Follow those rules when creating or modifying any `.github/workflows/*.yml` file.

The workflows in this package's `.github/` are inert until the monorepo CI spec replaces them.

## Build & Test

Run from the repository root (see the root `README.md` and `justfile`):

```sh
just adlc-setup                                         # npm ci + build:types + every frontend build
npm test -w @netray-info/common-frontend                # vitest
npm run build:types -w @netray-info/common-frontend     # emit dist/types
```
