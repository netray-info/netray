#!/usr/bin/env bash
# specs/features/v2-dkim-legacy-urls, requirement 11 (C5, C21): every production impl of
# `DomainRuns` (tracked crates/*/src/**/*.rs) defines `fn start_with` inside its own impl block, so
# no implementation drops DKIM selectors by inheriting the trait's default. The scan is a function
# over file paths (a small brace matcher, comments and strings skipped), self-tested on planted
# files. The trait definition itself (`pub trait DomainRuns`) has no impl and is ignored.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

# offenders <file>...: print "<file>:<line>" for each `impl ... DomainRuns for` block without `fn start_with`.
offenders() {
    python3 -I -c '
import re, sys
IMPL = re.compile(r"\bimpl\b(?:<[^{;]*?>)?\s+(?:[A-Za-z_][A-Za-z0-9_]*::)*DomainRuns\s+for\b")
def strip(s):
    out, i, n = [], 0, len(s)
    while i < n:
        if s.startswith("//", i):
            j = s.find("\n", i); j = n if j < 0 else j
            out.append(" " * (j - i)); i = j
        elif s.startswith("/*", i):
            j = s.find("*/", i + 2); j = n if j < 0 else j + 2
            out.append(re.sub(r"[^\n]", " ", s[i:j])); i = j
        elif s[i] == "\"":
            j = i + 1
            while j < n and s[j] != "\"":
                j += 2 if s[j] == "\\" else 1
            j = min(j + 1, n)
            out.append("\"" + re.sub(r"[^\n]", " ", s[i + 1:j - 1]) + "\""); i = j
        else:
            out.append(s[i]); i += 1
    return "".join(out)
for path in sys.argv[1:]:
    src = strip(open(path, encoding="utf-8").read())
    for m in IMPL.finditer(src):
        start = src.find("{", m.end())
        line = src.count("\n", 0, m.start()) + 1
        if start < 0:
            print(f"{path}:{line}"); continue
        depth, i = 0, start
        while i < len(src):
            if src[i] == "{": depth += 1
            elif src[i] == "}":
                depth -= 1
                if depth == 0: break
            i += 1
        if not re.search(r"\bfn\s+start_with\b", src[start:i]):
            print(f"{path}:{line}")
' "$@"
}

scratch=$(mktemp -d) || exit 1
trap 'rm -rf "$scratch"' EXIT
cat > "$scratch/bad.rs" <<'RS'
impl DomainRuns for A {
    fn start(&self) {}
}
impl DomainRuns for B {
    fn start(&self) {}
    fn start_with(&self) {}
}
RS
cat > "$scratch/bad_path.rs" <<'RS'
// fn start_with in a comment does not count
impl netray_web::api::DomainRuns for A {
    fn start(&self) {}
}
impl B { fn start_with(&self) {} }
RS
cat > "$scratch/good.rs" <<'RS'
impl DomainRuns for A {
    fn start(&self) {}
    fn start_with(&self, s: &[u8]) { if x { } }
}
impl crate::api::DomainRuns for B {
    fn start_with(&self) {}
}
RS
cat > "$scratch/trait.rs" <<'RS'
pub trait DomainRuns {
    fn start(&self);
    fn start_with(&self) { self.start() }
}
RS
[ "$(offenders "$scratch/bad.rs")" = "$scratch/bad.rs:1" ] || fail "self-test: an impl lacking start_with was not the one flag"
[ "$(offenders "$scratch/bad_path.rs")" = "$scratch/bad_path.rs:2" ] || fail "self-test: a path-prefixed impl lacking start_with was not flagged"
[ -z "$(offenders "$scratch/good.rs")" ] || fail "self-test: impls defining start_with were flagged"
[ -z "$(offenders "$scratch/trait.rs")" ] || fail "self-test: a trait definition was flagged"

files=$(git ls-files 'crates/*/src/*.rs' 'crates/*/src/**/*.rs' | grep -v '/tests/' | sort -u)
if [ -z "$files" ]; then
    fail "no tracked production Rust files found"
else
    while IFS= read -r hit; do
        [ -n "$hit" ] && fail "$hit: impl DomainRuns without fn start_with (it would inherit the default and drop DKIM selectors)"
    done < <(offenders $files)
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_domain_runs_selectors"
