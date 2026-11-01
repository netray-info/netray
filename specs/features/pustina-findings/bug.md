# Bug: pustina-findings

Written by `adlc feature start --bug`. The first four sections are yours; diagnose fills the last three.

## Symptom

A batch of five defects observed on the result page of a DNSSEC-signed domain whose zone has no DS
record in the parent (report 9Mga0Ibv, engine 0.30.0, profile 5):

1. `dns.dnssec` reports `pass` although the `dns.ds` block is empty; `tls.dane_valid` meanwhile says
   "DANE check skipped (requires DNSSEC)", contradicting it.
2. `dns.dnssec_rollover` warns "KSK with key tag 48553 has no matching DS record — new KSK not yet
   published in parent zone" for the zone's only KSK; no rollover is in progress.
3. In dark mode chosen with the theme toggle on a light OS, the `netray` logo in the page header is
   dark glyphs on a dark background, nearly invisible.
4. Every re-check of an unchanged domain shows "RUN 1 · … · FIRST RUN" and the "First run recorded."
   panel, dated with the first recording (2026-10-28), never a history.
5. Lines in "N lines clean · checks that pass" show only status, id, title and protocol · RFC: no
   sentence on what the check verifies, no guide link, no evidence link.
6. The social card `/r/<id>/og.png` is a dark card (`#0d1220`) with a large empty middle where the
   score used to be.

## Expected

1. A signed zone without a DS in the parent makes `dns.dnssec` fail with a finding that the zone is
   signed but not anchored in its parent.
2. `dns.dnssec_rollover` warns about a KSK without DS only while a rollover is plausible (more than one
   KSK, or DS records present); a single KSK with no DS at all leaves it not applicable.
3. The header logo follows the page theme in all four combinations of OS scheme and toggle.
4. A re-check whose result equals the stored run says the domain is unchanged since the first
   recording and when it was last checked, not "first run".
5. Every passing line carries one sentence on what the check verifies, the catalogue's impact
   sentence, a guide link and an evidence link.
6. The social card uses the light palette and fills the former score slot with the protocol grades.

## Reproduction

1. Check a domain signed at its DNS provider but without DS at the registrar; read `dns.dnssec`,
   `dns.dnssec_rollover`, `tls.dane_valid` and the `dns.ds` block.
2. Open any result page on a light OS, choose Theme → dark; look at the header logo.
3. Check a domain, press "Re-check now" without changing the domain; read the run header.
4. Expand "lines clean" on any result page.
5. Open Share → Social card on any result page.

## Environment

Production netray.info, engine 0.30.0, profile 5, Safari on macOS. Example domain: any signed zone
without parent DS (test data uses example.com fixtures). Out of scope here: `email.dnsbl` passing
while Spamhaus refuses the resolver (by design, other zones answered) and the missing IPv6 egress on
the production host (infrastructure).

## Loop

<Filled by diagnose: the command or test that goes red on the symptom, and how it was run.>

## Hypotheses

<Filled by diagnose: three to five falsifiable hypotheses, each with the observation that would refute it.>

## Root cause

<Filled by diagnose: the cause with an anchor (file and line), and the evidence that settled it.>
