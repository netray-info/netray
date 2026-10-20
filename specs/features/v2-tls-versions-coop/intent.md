# Intent: v2-tls-versions-coop

Operator decisions of 2026-10-20: probe TLS protocol versions and weak cipher suites per port with
hand-built ClientHello messages (no second TLS library), put the results into evidence and board
G's "Protocols and features" grid data, grade legacy versions, weak suites and
Cross-Origin-Opener-Policy as new checks in scoring profile v4, keep the per-port design of
v2-tls-backend, and test offline against an in-process responder. The UI is lane A's.
