# CLAUDE.md — beacon frontend
Apply [frontend-rules](../../../specs/rules/frontend-rules.md) for all changes under frontend/.
The dist/ directory is embedded into the Rust binary via rust-embed at release build time.
@netray-info/common-frontend comes from the root npm workspace (packages/common-frontend);
install with `just adlc-setup` from the repository root.
