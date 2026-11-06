# CLAUDE.md — netray-web

Pages, templates, islands, API v1, admission, badge, OG card, and their tests. Repo-wide rules,
verbs and the gate: the root [`CLAUDE.md`](../../CLAUDE.md).

## Server, API v1, admission

- Layer order, outermost first: concurrency limit, `request_id`, security headers, CORS, body limit, trace, compression, so preflights and 413s carry the request id and headers.
- Test invalid input through the router, not the handler: extractor rejections and unmatched paths answer first. In axum 0.8 `/x/{*rest}` conflicts with `/x/{id}`; a static segment beside a catch-all (`/r/{id}/og.png` with `/r/{id}/{*rest}`) does not.
- `openapi_responses` needs a 2xx and a non-2xx case per documented operation; a 2xx needing state runs its producer first (`{dnskey}`).
- Compose each new inspector onto the finished v1 document (`api/compose.rs`, one step per part); composing it with earlier parts renames a committed component and fails `openapi_contract`.
- An inspector under `/api/v1` runs on its module's `[modules.<x>]` state; `[api.<x>]` holds only per-client limits (email adds its slots). Never build a second inspector state.
- A cost above a limiter's burst never passes: refuse it as input (`CostExceedsLimit`, `INVALID_INPUT`) against the stored bursts before charging, never as `429`.
- governor's `check` spends a token whatever follows: take a keyed limiter after every refusal that must cost nothing.
- An ip answer validates, then checks the seam, then charges: a 400 or 503 costs nothing; charge the per-IP limiter only through `charge_caller` (feeds `X-RateLimit-Remaining` via a task-local).
- `api::ip_host` serves `ip.`'s V1 paths (`/country`, `/json`, `/ip/cidr` ...) and hands every other request back; the cutover mounts it before `ip_successor`.
- A golden over the DNS routes zeroes `latency_ms` and sorts hash-map-ordered lists (authcompare's servers); capture it twice before committing.
- An unguessable key comes from `Uuid::new_v4()`: a v7's last 48 bits hold 16 counter bits shared by every key of one millisecond.
- `metrics_util` 0.20's `Snapshotter::snapshot()` zeroes every value on read: read once, accumulate, or render a `PrometheusRecorder`. A `describe_*!` alone renders no HELP line: also register the metric at startup.

## Runs, admission and the store in tests

- The apex records a run after its response: a test reading `history.db` or the record-error counter polls with a bound or awaits `state.runs.drain()`.
- Admission starts one run per domain per 60 s on tokio's clock: a second run of one domain needs `common::pass_throttle()` (current_thread) or another domain.
- A paused tokio clock times out sqlx pool acquires: never `start_paused` with a `HistoryStore`; `pause()` only after store work is drained (`result_run_c15`).
- `HistoryStore::sweep` always keeps the head's predecessor: a swept-predecessor test needs a third entry as head.
- A refusal test over the badge or OG route asserts no run started: an incomplete run renders the same `?` and short Cache-Control.
- A page never fetches `/badge/<domain>.svg` unasked: the apex starts a run for a stale head on that request; previews carry `data-src` until the dialog opens.
- A recorded run has no caller DKIM selectors: a fixture's DKIM key sits under `default` or a provider selector.

## Pages and templates

- `pages/inspect.rs` and `pages/inspect/` name no module crate: reach module data through `api::jobs` helpers or a seam trait (`DnsResults`).
- A protocol view (`pages/views/`) claims a block only when it renders every key; each view adds `views_nothing_lost.rs` rows with distinct display texts.
- A part view (`views::render_part`) claims a whole inspector part: its fragment replaces the first claimed block; DNS reads `dns.answers`/`dns.servers` from `netray_dns::*_page_blocks`.
- The IP inspector renders blocks (`ip.lookup` card, `ip.headers` GET caller only); the IP view claims `ip.addresses` only to append the MaxMind attribution.
- Inspector parts stream in completion order: read each part's own `inspect-part-<n>` fragment; a bare `handshake` also matches `tls.handshake@` ids.
- Pages show values through `evidence::display_value`; `format_value` stays raw for exports, compact tables, links and explore.
- A row link on the stored result page uses `Links::Entry`; `Links::Live` anchors exist only on the live run page.
- The score under the letter goes through `views::score_number`.
- A change to the social card's drawing bumps `CARD_RENDERING_VERSION` (`og/layout.rs`), or clients revalidate the old card to 304.
- Absolute URLs come from `pages::ORIGIN`; in-page links are apex-relative, except the header logo (links to `ORIGIN`).
- askama 0.16: close every `{% call %}` with `{% endcall %}`; it escapes `'&<>"` as `&#39;` `&#38;` `&#60;` `&#62;` `&#34;`, so tests over rendered text compare that form.
- An optional element in a golden-tested template opens with `{%- if %}`, so the markup without it stays byte-identical.
- A `base.html` block default compiles against every child: frame-struct fields only inside blocks a struct-less child overrides.
- A new line in `result_evidence.html` changes every islands fixture: put a conditional on an existing line, regenerate with `UPDATE_GOLDEN=1`.
- A golden over live-run markup carries the wall clock: normalise it (`NOW` in `tests/it/result_islands_fixtures.rs`).
- The expired-entry page inlines its sheet (`entries/render.rs` `stylesheet`): a frame rule it needs lives in `pages.css`.

## CSS and the gate

- The CSS gate refuses literal font sizes and colours: add a token to `tokens.json` (build.rs emits the variable).
- The gate keeps a component's classes in its own stylesheet: a page rule on `.check-row` or `.query-input__*` goes into that component's CSS or uses a structural selector.
- Identifier-valued properties (`grid-area`, `transition`) go into `IDENTIFIER_PROPERTIES` (`src/gate.rs`), or the gate refuses colour words in them.
- The gate's colour words include CSS system colours (`menu`, `canvas`, `field`): no island or class of that name. A class starting `grade-` belongs to the grade component.
- The colour gate reads any island-test string holding `;` as CSS: write a newline in test markup as `\n`, never `&#10;`.
- Page tests assert a component by its markup (`class="diff-block"`), never the bare class name: pages inline every component stylesheet.
- A new table inside `main` inherits `main th, main td`: override colour and borders in its own rules.
- A too-wide evidence table scrolls inside `.evidence-block` (`overflow-x: auto`); never `overflow-wrap: anywhere` on cells.
- A browser breaks after a hyphen: to break a domain only at dots, wrap each label in an inline-block (`.result-hero__label`).
- Spectrum labels are placed twice, desktop (`labels`) and phone (`phone_labels`, 358 px); the phone media query overrides with `!important`, so a new label style needs its phone twin.
- The spectrum follows the grade: `spectrum::layout` mutes a section with a weighted unmeasured check and sums M as `scoring.rs` does; change both together.

## Islands

- An island that reads the inspector's host field re-reads it before acting: the shell fills it without an input event.
- Node 26's global `localStorage` shadows happy-dom's: an island test installs its own storage (`islands/test/theme.test.ts` `useStorage`).
- Island fixture goldens embed server text verbatim: keep them LF.

## Design boards (`just design-check`)

- Needs the private canvas; its export serves boards as sandboxed blob: iframes: use Playwright's `contentFrame()`, never `iframe.contentDocument`.
- Live TLS boards need network: copy the build to the main checkout's `target/debug/netray` (back up, `rm`, `cp`, restore) and pass `NETRAY_BIN`.
- Local DNS is blocked: screenshot DNS views from `cargo run -p netray-web --example dns_stub [port]`.
- A board's interpolated text sits in `span.sc-interp`: select its parent with `:has(> span:text-is(…))`; write a regex digit as `\\\\d` in the manifest's JSON.
- The design fixtures' stored grades must equal `score_run`'s, or the score guard hides the score on every gated page.
- A board span in a flex column can read `height: auto`: measure one-line text through its container's height.
- The 1 px typography tolerance cannot tell 11 from 12 px: pin a board size through its own token, never a label token a `*Phone` value lowers.
- A board with `wait` marks its always-rendered elements `"shell": true`; without a non-shell element the board is `no state`.
- A state behind a control is measured with the manifest's `click` (`{"board": …, "page": …}`).
