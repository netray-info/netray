# Contributing

## Prerequisites

- Rust toolchain (stable)
- Node.js 22+

## Development

```sh
# Two terminals:
make dev            # Rust server on :8080
make frontend-dev   # Vite on :5173, proxies /api/* to :8080

# Run before pushing:
make ci             # lint + test + frontend + build
```

## Conventions

Follow `CLAUDE.md` for coding conventions, engineering principles, and commit rules.

Key points:
- No formatting changes mixed with logic changes
- No speculative abstractions or config options without a current caller
- Keep changes scoped — don't touch unrelated modules

## Pull requests

- Keep PRs focused on a single concern
- Separate formatting-only commits from functional changes
- Reference the relevant GitHub issue if one exists

## Issues

Use GitHub issues for bug reports and feature requests.
