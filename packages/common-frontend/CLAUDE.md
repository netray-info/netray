# CLAUDE.md — netray-common-frontend

## Rules

- Do NOT add a `Co-Authored-By` line for Claude in commit messages.
- Don't add heavy dependencies for minor convenience — check if existing deps already cover the need.
- Don't mix formatting-only changes with functional changes in the same commit.
- Don't modify unrelated modules "while you're in there" — keep changes scoped.
- Don't add speculative flags, config options, or abstractions without a current caller.
- Don't bypass failing checks (`--no-verify`, `#[allow(...)]`) without explaining why.
- Don't include PII, real email addresses, or real domains (other than example.com) in test data, docs, or commits.
- If uncertain about an implementation detail, leave a concrete `TODO("reason")` rather than a hidden guess.

## Project Overview

**netray-common-frontend** is a shared SolidJS package (`@netray-info/common-frontend`) published to GitHub Packages. It provides the theme system, shared UI components, keyboard utilities, and CSS design tokens consumed by all tool frontends.

- **Registry**: GitHub Packages (`https://npm.pkg.github.com`, scope `@netray-info`)
- **Consumers**: ifconfig-rs, mhost-prism, tlsight, lens frontends

## CI/CD

Workflow rules: [`specs/workflow-rules.md`](../specs/workflow-rules.md) in the netray.info meta repo. Follow those rules when creating or modifying any `.github/workflows/*.yml` file.

Workflows: `ci.yml` (PR gate: lint, test, audit), `publish.yml` (tag-push: test → publish to GitHub Packages).

Publishing: tag push matching `v*.*.*` triggers `publish.yml`. Manual `workflow_dispatch` also supported. `NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` is required on every `npm ci` and `npm publish` step.

## Build & Test

```sh
npm ci                  # install deps (requires NODE_AUTH_TOKEN if private deps)
npm run lint            # ESLint / type check
npm test                # vitest
npm run build           # compile to dist/
```
