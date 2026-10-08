#!/usr/bin/env bash
# C1, C2, C3, C5, C7-C10: one CI workflow and one release workflow at the repo root, pinned actions,
# no per-crate workflows. Parsed with ruby's YAML (PyYAML is not installed).
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

command -v ruby >/dev/null 2>&1 || fail "ruby not found"

out=$(ruby -ryaml -e '
fails = []
ci_f = ".github/workflows/ci.yml"
rel_f = ".github/workflows/release.yml"

# C7: exactly these two tracked workflow files
tracked = `git ls-files ".github/workflows/*"`.split("\n").sort
fails << "tracked workflows are #{tracked.inspect}, want [#{ci_f}, #{rel_f}]" unless tracked == [ci_f, rel_f]

# C5: no per-crate/package workflows
stray = `git ls-files`.split("\n").grep(%r{^(crates|packages)/.*/\.github/})
fails << "per-crate .github still tracked: #{stray.first(3).join(" ")}#{stray.size > 3 ? " ..." : ""}" unless stray.empty?

def load_wf(f, fails)
  return [nil, nil, nil] unless File.file?(f)
  raw = File.read(f)
  doc = YAML.safe_load(raw, aliases: true)
  unless doc.is_a?(Hash)
    fails << "#{f} is not a YAML mapping"
    return [raw, nil, nil]
  end
  on = doc.key?("on") ? doc["on"] : doc[true]
  on = { on => nil } if on.is_a?(String)
  on = on.each_with_object({}) { |k, h| h[k] = nil } if on.is_a?(Array)
  [raw, doc, on || {}]
end

def steps_of(doc)
  (doc["jobs"] || {}).values.flat_map { |j| j.is_a?(Hash) ? (j["steps"] || []) : [] }.select { |s| s.is_a?(Hash) }
end

def text_of(s) = [s["run"], s["uses"], s["with"].to_s, s["name"]].compact.join(" ")

# C8 / C1: ci.yml
raw, ci, on = load_wf(ci_f, fails)
if raw.nil?
  fails << "#{ci_f} missing"
elsif ci
  push = on["push"]
  branches = push.is_a?(Hash) ? Array(push["branches"]) : []
  fails << "ci.yml: on.push.branches does not contain main" unless branches.include?("main")
  fails << "ci.yml: on has no pull_request" unless on.key?("pull_request")
  jobs = (ci["jobs"] || {}).values.select { |j| j.is_a?(Hash) }
  fails << "ci.yml: no job runs-on ubuntu-24.04-arm" unless jobs.any? { |j| Array(j["runs-on"]).include?("ubuntu-24.04-arm") }
  runs = steps_of(ci).map { |s| s["run"].to_s }.join("\n")
  fails << "ci.yml: no step runs `just adlc-setup`" unless runs.include?("just adlc-setup")
  fails << "ci.yml: no step runs `just check`" unless runs.include?("just check")
  conc = ci["concurrency"]
  fails << "ci.yml: top-level concurrency.cancel-in-progress is not true" unless conc.is_a?(Hash) && conc["cancel-in-progress"] == true
  # check-sitemap dates pages by git history: a shallow checkout breaks it.
  co = (ci["jobs"] || {}).values.flat_map { |j| Array(j["steps"]) }.find { |s| s["uses"].to_s.start_with?("actions/checkout@") }
  fails << "ci.yml: checkout does not fetch the full history (fetch-depth: 0)" unless co && co.dig("with", "fetch-depth").to_s == "0"
  # The data image also carries tracked files (asn_patterns.toml); CI must test the committed ones.
  fails << "ci.yml: GeoIP data copy may overwrite tracked files (copy without clobbering)" if raw =~ %r{docker cp \S+ crates/ifconfig-rs/data} || raw !~ /cp (-n|--no-clobber|--update=none)/
end

# C9 / C2: release.yml
raw, rel, on = load_wf(rel_f, fails)
if raw.nil?
  fails << "#{rel_f} missing"
elsif rel
  tags = on["push"].is_a?(Hash) ? Array(on["push"]["tags"]) : []
  fails << "release.yml: on.push.tags has no pattern starting with v" unless tags.any? { |t| t.to_s.start_with?("v") }
  fails << "release.yml: on has no workflow_dispatch" unless on.key?("workflow_dispatch")
  # Two runs for one tag must not both pass the existence check: queue them per ref.
  rconc = rel["concurrency"]
  fails << "release.yml: no per-ref concurrency group that queues (cancel-in-progress: false)" unless rconc.is_a?(Hash) && rconc["group"].to_s.include?("github.ref") && rconc["cancel-in-progress"] == false
  # A branch named like a tag must not publish: the run refuses any ref that is not a tag.
  fails << "release.yml: does not refuse non-tag refs (GITHUB_REF_TYPE)" unless raw.include?("GITHUB_REF_TYPE")
  jobs = (rel["jobs"] || {}).values.select { |j| j.is_a?(Hash) }
  fails << "release.yml: no job runs-on ubuntu-24.04-arm" unless jobs.any? { |j| Array(j["runs-on"]).include?("ubuntu-24.04-arm") }
  fails << "release.yml: linux/arm64 not present" unless raw.include?("linux/arm64")

  perms = ([rel["permissions"]] + jobs.map { |j| j["permissions"] }).compact
  allowed = { "contents" => "read", "packages" => "write" }
  if perms.empty?
    fails << "release.yml: no permissions block"
  else
    bad = perms.reject { |p| p.is_a?(Hash) && p.all? { |k, v| allowed[k] == v } }
    fails << "release.yml: permissions outside {contents: read, packages: write}: #{bad.inspect}" unless bad.empty?
    union = perms.select { |p| p.is_a?(Hash) }.reduce({}) { |a, p| a.merge(p) }
    fails << "release.yml: permissions union is #{union.inspect}, want #{allowed.inspect}" unless union == allowed
  end

  %w[latest deploy curl webhook].each do |w|
    re = w == "latest" ? /\blatest\b/i : /#{w}/i
    fails << "release.yml: contains forbidden word #{w}" if raw =~ re
  end

  fails << "release.yml: image ref ghcr.io/netray-info/netray: missing" unless raw.include?("ghcr.io/netray-info/netray:")
  fails << "release.yml: no leading-v strip (#v) found" unless raw.include?("#v")

  steps = steps_of(rel)
  texts = steps.map { |s| text_of(s) }
  i_check = texts.index { |t| t =~ /imagetools inspect|manifest inspect/ }
  i_build = texts.index { |t| t =~ /docker build|buildx build|build-push-action|docker\/build/ }
  subs = %w[lens dns tls http email ip site]
  i_smoke = texts.index { |t| t.include?("docker run") && subs.all? { |c| t =~ /(^|[^a-z])#{c}([^a-z]|$)/ } }
  i_push = texts.rindex { |t| t =~ /push/i }
  fails << "release.yml: no registry-exists check step (imagetools/manifest inspect)" if i_check.nil?
  fails << "release.yml: no build step" if i_build.nil?
  fails << "release.yml: no smoke step (docker run + all subcommands)" if i_smoke.nil?
  fails << "release.yml: no push step" if i_push.nil?
  fails << "release.yml: registry check does not come before build" if i_check && i_build && i_check >= i_build
  fails << "release.yml: smoke step does not come before push step" if i_smoke && i_push && i_smoke >= i_push
end

# C10 / C3: every uses: is pinned to a 40-hex SHA with a version comment
[ci_f, rel_f].each do |f|
  next unless File.file?(f)
  File.readlines(f).each_with_index do |l, n|
    next unless l =~ /^\s*(-\s*)?uses:/
    unless l =~ %r{uses:\s*[\w.-]+/[\w.-]+(/[\w./-]+)?@[0-9a-f]{40}\s+#\s*v}
      fails << "#{f}:#{n + 1}: unpinned action: #{l.strip}"
    end
  end
end

puts fails
') || fail "ruby check crashed: $out"

if [ -n "$out" ]; then
    echo "$out" | sed 's/^/FAIL: /'
    exit 1
fi

echo "ok"
