# Plan: v2-ci-timeout

## Phase 1 — Raise the check job's timeout

## Groups

- G1: C1, C2, C3

## Plan

- G1: `.github/workflows/ci.yml`, job `check`, key `timeout-minutes`: 30 → 45.
