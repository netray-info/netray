// Redirect handling is built into request.rs via reqwest's custom redirect policy.
// This module exists for the SDD file structure but the logic is in request.rs.
//
// The custom redirect policy in request.rs:
// 1. Captures each hop (URL, status, location, http_version)
// 2. Enforces the max_redirects limit
// 3. Sets redirect_limit_reached flag when limit is hit
