
## f8ecf55d..d9aa2d11

### Reader

COUNTS blockers=1 majors=1 minors=1
LENSES Engineering, Testing
BLOCKER | crates/web/src/pages/views/validity.rs:58 | The validity bar draws a leaf that expired less than 24 hours ago as valid ("today · 0 days left", pass-tone elapsed bar, amber caption shown), while the same section's Subject cell carries the fail chip "expired". | The backend computes `days_remaining = (not_after - now) / 86400`, which truncates toward zero, and emits `expired = is_expired || lapsed`. A leaf with not_after 2026-10-21T00:00:00Z, measured at 05:00Z, is therefore stored with `days_remaining: 0, expired: true`. The view sets now = end − 0·86400 = end, so `now > end` is false and the state is Valid. Requirement 6 says an expired leaf draws the whole bar in the fail tone with "expired <n> days ago". Nothing in `certificate()` consults `expired`. The test fixture (`views_tls_validity.rs` `blocks`) never builds a leaf that lapsed under a day, so no test catches this.
MAJOR | crates/web/src/pages/views/tls.rs:601 | The Protocols grid prints "ALPN none" when no handshake of the port was taken, even though the run negotiated an ALPN protocol and the untaken handshake block, rendered generically next to it, shows it. | Take the spec's own Phase 1 case: a stored `tls.chain@192.0.2.1:443` holding an unknown key, plus `tls.handshake@192.0.2.1:443` with `alpn: h2`, `tls.protocols` with `tls1_3: enabled`, and an `ok` row in `tls.addresses`. The chain is not taken. The handshake is taken only beside a taken chain (line 135), so `port.handshakes` is empty. `tls.protocols` is still taken, so the grid renders, and `per("alpn")` over no handshakes yields "none" with the `alpn_consistency` tone. Every other handshake-backed cell (ECH, Key exchange, SNI, STARTTLS) is left out when absent; only ALPN states an unmeasured fact (Rule M). The backend writes no such data today (no key was ever removed from REGISTER), but the spec names this input in scope.
MINOR | crates/web/islands/src/ports.ts:33 | The port chooser presses 443 for a bare IPv6 input that the server reads as host `2001:db8::` with a port list, so the chooser shows a different port than the run inspects. | Type `2001:db8::1` (or `2606:4700::1111`). The server's `split_target_and_ports` takes the digits after the last colon as ports, and `parse_target("2001:db8::")` is a valid IP, so the job inspects [2001:db8::]:1. The island's `target()` sees a colon left in the host, returns ports null and presses 443. `ports.test.ts:12` pins this reading. Spec requirement 15 says the chooser reads a suffix only as the server's parser does. The input is out of scope: the server's documented forms require IPv6 in brackets.

```quote crates/web/src/pages/views/validity.rs:58
    let state = if now > end {
```

```quote crates/web/src/pages/views/tls.rs:1020
            unix_time(na).map(|end| end - left * 86_400)
```

```quote crates/tls/src/tls/chain.rs:90
    let days_remaining = (not_after_ts - now_ts) / 86400;
```

```quote crates/tls/src/module.rs:1063
            EvValue::Bool(cert.is_expired || lapsed),
```

```quote crates/web/src/pages/views/tls.rs:601
        "none".to_string()
```

```quote crates/web/src/pages/views/tls.rs:135
                    .filter(|(addr, _)| chains.iter().any(|(a, _, _)| a == addr))
```

```quote crates/web/islands/src/ports.ts:33
  if (host.includes(":")) return { host: v, ports: null, fixed: false };
```

### Spec reader

COUNTS blockers=0 majors=6 minors=1
REQUIREMENTS met=13 partial=6 missed=0 contradicted=0
MAJOR | crates/web/src/pages/views/mod.rs:25 | `ViewContext` does not gain the sibling blocks. They travel as a separate `siblings` argument of a new `render_with`, while the struct keeps only statuses, decorate and feeds. Behaviour is the same; the struct the requirement names is not changed. | R1: any caller holding a `ViewContext` (e.g. `views::render(block, &ctx)`) passes `&[]` siblings, and the TLS view then sees the block alone
MAJOR | crates/web/src/pages/views/tls.rs:858 | Handshake facts are grouped over taken handshakes only, so a fact that a failed address lacks still shows once with no address. | R3: a port with A `ok` 22 ms and B `HANDSHAKE_TIMEOUT` has `port.handshakes.len()==1`, so `distinct` sets `partial=false`, and the note reads "Port 443 · handshake 22 ms", not "22 ms (A)"
MAJOR | crates/web/static/inspector_shell.css:222 | A port whose lines all pass gets `data-tone="pass"` on an empty meta span, and no CSS rule styles it. The tab shows no pass tone, while warn and fail are coloured. | R14: a 443 port with every line passing renders `<span class="inspector-ports__meta" data-tone="pass"></span>`, which is visually identical to a toneless tab
MAJOR | crates/web/islands/src/ports.ts:93 | A suffix that the server's parser refuses still presses 443, because `ports: null` falls back to `[443]`. | R15: typing `example.com:0` gives `target()` → `{ports:null, fixed:true}`, `sync()` takes `list=[443]`, and the 443 toggle shows `aria-pressed="true"`, where the requirement says "presses nothing"
MAJOR | crates/web/templates/partials/tls_certificate.html:1 | The changed-certificate banner mounts inside the Certificate section's card, below its `<h3>`, not above the section. | R17: a one-chain port with a stored snapshot of another SHA-256 appends the banner into `.tls-certdiff`, the first child of `.protocol-view__card` inside `<section id="ev-tls.chain@…">`
MAJOR | specs/features/v2-tls-ui/report.md:274 | The design gate does not measure the related box, and the stored three-ports-with-STARTTLS state was never screenshotted or read beside G. | R19: the manifest rows omit the related box, and the visual reading covers only the live 1.1.1.1 runs, the differing leaves and the expired leaf
MINOR | crates/web/islands/src/ports.ts:72 | The chooser adds an "Enter a host first." note on a toggle press with an empty host field. No requirement names this note. | Unrequested: a 8443 press with an empty field shows the note and leaves the field empty

```quote specs/features/v2-tls-ui/spec.md:98
1. **Port view seam.** `ViewContext` gains the sibling blocks the renderer shows with the block
```

```quote crates/web/src/pages/views/mod.rs:25
pub struct ViewContext<'a> {
    pub statuses: &'a [(CheckId, Status)],
    pub decorate: Decorate<'a>,
    pub feeds: &'a [String],
}
```

```quote specs/features/v2-tls-ui/spec.md:113
3. **Distinct values.** Where one cell shows a fact several addresses report (handshake facts,
   OCSP, SCTs), equal values held by every address show once; otherwise (values differ, or some
   addresses do not hold the fact) each value is followed by its addresses in parentheses.
```

```quote crates/web/src/pages/views/tls.rs:858
    let per = |key: &str, show: &dyn Fn(&Value) -> String| {
        distinct_texts(
            port.handshakes
                .iter()
                .filter_map(|(addr, _, f)| get(f, key).map(|v| (*addr, show(v))))
                .collect(),
            port.handshakes.len(),
        )
    };
```

```quote specs/features/v2-tls-ui/spec.md:207
    a port whose lines pass carries the pass tone; both arrive with the part. The band port's tab is current. The `tabs` island
```

```quote crates/web/src/pages/inspect.rs:1430
            match (fails, warns, unmeasured, passes) {
                (0, 0, 0, 0) => None,
                (0, 0, 0, _) => Some("pass"),
```

```quote crates/web/static/inspector_shell.css:222
.inspector-ports__meta {
  font-weight: 400;
}
.inspector-ports__meta[data-tone="warn"] {
  color: var(--status-warn-text);
}
.inspector-ports__meta[data-tone="fail"] {
  color: var(--status-fail-text);
}
```

```quote specs/features/v2-tls-ui/spec.md:219
    only as the server's parser does (a list it would refuse presses nothing). The extra-port field
```

```quote crates/web/islands/src/ports.ts:31
  if (!ports || colon === 0) return fixed;
```

```quote crates/web/islands/src/ports.ts:93
    const list = current.ports ?? [443];
```

```quote specs/features/v2-tls-ui/spec.md:237
    run of the key, when the SHA-256 differs, a banner from a server-rendered `template` above the
    section names the changes: "expiry <old> → <new>", "issuer <old> → <new>", "names added: …",
```

```quote crates/web/templates/partials/protocol_view.html:1
<section class="protocol-view" id="ev-{{ id }}" data-block="{{ id }}"><h3 class="protocol-view__title">{{ title }}</h3>
<div class="protocol-view__card">{{ body|safe }}
```

```quote crates/web/templates/partials/tls_certificate.html:1
{% if let Some(d) = certdiff %}<div class="tls-certdiff" data-island="certdiff" data-cert-key="{{ d.key }}" data-not-after="{{ d.not_after }}" data-issuer="{{ d.issuer }}" data-sans="{{ d.sans }}" data-sha256="{{ d.sha256 }}" hidden><template class="tls-certdiff__template"><div class="tls-certdiff__banner" role="status"><p>Certificate changed since your last visit: <span class="tls-certdiff__changes"></span></p><button type="button" class="tls-certdiff__forget">Forget</button></div></template></div>
```

```quote specs/features/v2-tls-ui/spec.md:247
19. **Design gate.** The private manifest measures G's and GDark's port chooser, addresses table,
    certificate section (validity labels and bar, facts grid, chain pills, trust text), protocols
    grid and related box; `just design-check` exits 0, every row fixed or accepted. Screenshots at
    1280 and 390 px, both themes, of the live board target and of the stored states (two
    addresses with differing leaves, three ports with STARTTLS, an expired leaf) are read beside G
```

```quote specs/features/v2-tls-ui/report.md:274
name and state. The related box is not measured: the board's live target is an IP address
```

```quote specs/features/v2-tls-ui/report.md:328
with a STARTTLS port needs DNS for a mail host, blocked locally; the panels are verified with the
two-port live run and `inspect_tls_panels.rs`.
```

```quote crates/web/islands/src/ports.ts:72
    note.textContent = "Enter a host first.";
```

### Refutation

| finding | refuter | confidence | disposition |
|---|---|---|---|
| F1 BLOCKER validity.rs:58 expired leaf under a day draws valid | CONFIRMED | 9 | kept |
| F2 MAJOR tls.rs:601 ALPN none without a taken handshake | CONFIRMED | 8 | kept |
| F3 MAJOR views/mod.rs:25 siblings not in ViewContext | CONFIRMED | 8 | kept (spec wording) |
| F4 MAJOR tls.rs:858 failed address not named in distinct values | CONFIRMED | 8 | kept |
| F5 MAJOR inspector_shell.css:222 pass tone unstyled | UNCERTAIN | 5 | downgraded to MINOR |
| F6 MAJOR ports.ts:93 refused suffix presses 443 | CONFIRMED | 8 | kept |
| F7 MAJOR tls_certificate.html:1 banner inside the card | CONFIRMED | 6 | kept |
| F8 MAJOR report.md:274 related box unmeasured, STARTTLS state unread | CONFIRMED | 8 | kept |

### Refuted

None.

### Calibration

| finding | refuter answer | confidence | held | command |
|---|---|---|---|---|
| F1 | CONFIRMED | 9 | held | read validity.rs:58 with tls.rs:1020: days_remaining 0 sets now = end; `now > end` false |
| F2 | CONFIRMED | 8 | held | read tls.rs:595-605: `per("alpn")` empty pushes "none" |
| F3 | CONFIRMED | 8 | held | read views/mod.rs:25 |
| F4 | CONFIRMED | 8 | held | read tls.rs:858: group size is the taken handshakes |
| F5 | UNCERTAIN | 5 | held (minor) | read inspector_shell.css:222-230 |
| F6 | CONFIRMED | 8 | held | read ports.ts:93 with target("example.com:0") |
| F7 | CONFIRMED | 6 | held | read tls_certificate.html:1 inside protocol_view.html's card |
| F8 | CONFIRMED | 8 | held | read report.md:274 and :328 |

### Roll call

| principle | answer |
|---|---|
| P03 | convergence — no new status enum, no codename in added UI lines |
| P10 | convergence — a non-ok address renders as an error row; checks keep their verdicts |
| P12 | absence — no outbound call or module entry added |
| P13 | absence — no limiter or route changed |
| P18 | absence — no reqwest client or config struct added |
| P26 | absence — no check id added |
| P35 | absence — no rule under specs/rules changed |
| P36 | convergence — no generated file changed without its source |
| P40 | absence — no module added or removed |

### Summary

Before refutation: reader 1/1/1, spec reader 0/6/1 (REQUIREMENTS met=13 partial=6 missed=0
contradicted=0). After refutation: blockers 1, majors 6, minors 3 (F5 downgraded); verified 8, held
8. Roll call: answered 9, convergence 3, divergence 0, absence 6.

### Dispositions

| finding | disposition |
|---|---|
| F1 BLOCKER | fixed on this branch: an `expired` leaf's run moment lies past not_after; test `a_leaf_lapsed_within_the_day_draws_expired` |
| F2 MAJOR | fixed: ALPN "none" only with a handshake read; test `no_alpn_cell_without_a_taken_handshake` |
| F3 MAJOR | requirement 1 amended: the siblings travel through `render_with` |
| F4 MAJOR | requirement 3 amended: distinct values range over addresses that completed a handshake; a failed address is its error row |
| F5 MINOR | fixed: the tab meta draws its tone as a bar |
| F6 MAJOR | fixed: a refused suffix presses nothing; test in `ports.test.ts` |
| F7 MAJOR | requirement 17 amended: the banner sits at the top of the certificate section |
| F8 MAJOR | requirement 19 amended: the related box needs a host target and STARTTLS needs DNS, neither available to the live board here |

## d9aa2d11..4eceffdb

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Testing
MAJOR | crates/web/islands/src/ports.ts:94 | The new "refused suffix" branch takes `fixed` to mean "the server refuses this input". But the island's mirror marks some inputs as `fixed` that the server accepts. For those inputs the port chooser now shows no port pressed while the run inspects 443. Before this change it showed 443 pressed. | Input `h = "[2001:db8::1]: 443"`. In the island, `portList(" 443")` fails the regex `/^\d+(,\d+)*$/` (ports.ts:3), so `target()` returns `{ports:null, fixed:true}` and `sync()` sets `list = []`. On the server `parse_ports` trims each element (input.rs:126) and accepts port 443. `"[2001:db8::1]:+443"` hits the same mismatch, because Rust's `u16` parse accepts a leading `+`.

```quote crates/web/islands/src/ports.ts:94
    const list = current.ports ?? (current.fixed && h.value.trim() ? [] : [443]);
```

```quote crates/web/islands/src/ports.ts:3
  if (!/^\d+(,\d+)*$/.test(list)) return null;
```

```quote crates/tls/src/input.rs:126
            let trimmed = p.trim();
```

### Spec reader

COUNTS blockers=0 majors=3 minors=0
REQUIREMENTS met=5 partial=3 missed=0 contradicted=0
MAJOR | crates/web/src/pages/views/tls.rs:1034 | When `expired` is true the bar's moment is `not_after + 1`, not requirement 6's `not_after − days_remaining`, and the label reads "expired today"; requirement 6 was not amended. | a leaf with `days_remaining` 0 and `expired` true (test `a_leaf_lapsed_within_the_day_draws_expired`)
MAJOR | crates/web/islands/src/ports.ts:95 | A suffix refused for too many ports keeps the previous selection pressed. | `example.com:1,2,3,4,5,6,7,8` with max 7 leaves 443 pressed; the server refuses it with TooManyPorts (crates/tls/src/input.rs:147)
MAJOR | crates/web/static/inspector_shell.css:226 | The tab tone bar was added after the reading, and the several-port run was recorded at 1280 light only. | report.md lists only `two-ports 1280 light`

```quote crates/web/src/pages/views/tls.rs:1032
        (Some(Value::Time(na)), Some(Value::Int(left))) => unix_time(na).map(|end| {
            let moment = end - left * 86_400;
            if lapsed { moment.max(end + 1) } else { moment }
        }),
```

```quote crates/web/islands/src/ports.ts:91
  const sync = (): boolean => {
    current = target(h.value);
```

```quote crates/web/static/inspector_shell.css:225
/* the port's worst status as a tone bar beside its name */
.inspector-ports__meta[data-tone]::before {
```

### Dispositions

| finding | refuter | disposition |
|---|---|---|
| ports.ts:94 trimmed and `+` items refused by the island | not refuted (verified: the island mirror and input.rs:126 read) | fixed: items trimmed and parsed as the server's u16; an unbracketed IPv6 host is refused as the server refuses it; tests in `ports.test.ts` |
| tls.rs:1034 requirement 6 unamended | not refuted (verified by reading spec.md:137-144) | requirement 6 amended: the backend's `expired` decides, "expired today" within the day |
| ports.ts:95 too many ports keeps a selection | not refuted (verified by reading `sync`) | fixed: nothing pressed, the note shows; test added |
| inspector_shell.css:226 tab bar unread | not refuted (verified: report listed 1280 light only) | read: two-port live run at 1280 and 390, light and dark, scroll width equals viewport; the tab shows name, tone bar and counts (scratchpad `tlsui/shots/two-ports-*`); a gap before the meta text added |

### Summary

Reader 0/1/0, spec reader 0/3/0 (REQUIREMENTS met=5 partial=3); all four verified and held, all
fixed or amended in the next commit. Roll call unchanged from the full-range reading.
