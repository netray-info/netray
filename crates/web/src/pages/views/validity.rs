//! The validity bar's geometry: a pure function of a certificate's validity period and now
//! (spec v2-tls-ui, requirement 6). x values are viewBox units of a bar 1000 wide.

use netray_tls::quality::checks::EXPIRY_WARN_DAYS;

use crate::evidence::unix_time;

const DAY: i64 = 86_400;
/// The bar's viewBox width.
pub const BAR_WIDTH: u32 = 1000;

/// Where now stands in a certificate's validity period.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ValidityState {
    Valid,
    Expired,
    NotYetValid,
}

impl ValidityState {
    /// The `data-state` value.
    pub fn as_str(self) -> &'static str {
        match self {
            ValidityState::Valid => "valid",
            ValidityState::Expired => "expired",
            ValidityState::NotYetValid => "not-yet-valid",
        }
    }
}

/// The bar of one validity period: whole days, and the marker and warn-zone start in viewBox
/// units. `days_left` is negative once expired, `elapsed_days` before the period starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidityLayout {
    pub total_days: i64,
    pub elapsed_days: i64,
    pub days_left: i64,
    pub warn_days: i64,
    pub marker_x: u32,
    pub warn_x: u32,
    pub state: ValidityState,
}

/// The layout of the period `not_before`..`not_after` (RFC 3339) at `now` (Unix seconds);
/// `None` when a time does not parse or the period is empty.
pub fn layout(not_before: &str, not_after: &str, now: i64) -> Option<ValidityLayout> {
    let (start, end) = (unix_time(not_before)?, unix_time(not_after)?);
    let total_days = (end - start).div_euclid(DAY);
    if total_days < 1 {
        return None;
    }
    let elapsed_days = (now - start) / DAY;
    let x = |days: i64| {
        (days as f64 / total_days as f64 * f64::from(BAR_WIDTH))
            .round()
            .clamp(0.0, f64::from(BAR_WIDTH)) as u32
    };
    let state = if now > end {
        ValidityState::Expired
    } else if now < start {
        ValidityState::NotYetValid
    } else {
        ValidityState::Valid
    };
    Some(ValidityLayout {
        total_days,
        elapsed_days,
        days_left: (end - now) / DAY,
        warn_days: EXPIRY_WARN_DAYS,
        marker_x: x(elapsed_days),
        warn_x: x(total_days - EXPIRY_WARN_DAYS),
        state,
    })
}
