# Intent: v2-tls-backend

## Problem

The V2 TLS inspector drops every port but one, the run's warnings and skipped addresses, and the
certificate and handshake details V1 showed (inventory theme E, rows 3.3, 3.5, 3.9, 3.16, 3.19,
3.23-3.25, 3.44); `/api/v1/tls/inspect` refuses `ports`.

## Proposed outcome

The backend data for theme E exists: per-port outcomes, notices, detail fields, live OCSP, `ports`
in the API. The UI half renders it later (lane A).
