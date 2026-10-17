//! `/status`: the server-side skeleton the `status` island fills (v2-pages requirement 25).

use askama::Template;
use axum::http::StatusCode;
use axum::response::Response;

use super::{Frame, page};

#[derive(Template)]
#[template(path = "pages/status.html")]
struct StatusPage {
    frame: Frame,
}

/// `GET /status`.
pub(super) async fn show() -> Response {
    let frame = Frame::new(
        "Service Status - netray",
        "Live uptime of the netray.info services, sourced from UptimeRobot.",
        "/status",
    );
    page(
        StatusCode::OK,
        "public, max-age=3600",
        &StatusPage { frame },
    )
}
