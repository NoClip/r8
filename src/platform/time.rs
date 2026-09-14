//! Safe Rust reimplementation of Google V8's `src/base/platform/time.h`.
//!
//! Provides `TimeDelta`, wall-clock `Time`, monotonic `TimeTicks`, and high-resolution
//! timing primitives matching V8 semantics.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct TimeConstants;

impl TimeConstants {
    pub const HOURS_PER_DAY: i64 = 24;
    pub const MILLISECONDS_PER_SECOND: i64 = 1000;
    pub const MILLISECONDS_PER_DAY: i64 = Self::MILLISECONDS_PER_SECOND * 60 * 60 * Self::HOURS_PER_DAY;
    pub const MICROSECONDS_PER_MILLISECOND: i64 = 1000;
    pub const MICROSECONDS_PER_SECOND: i64 = Self::MICROSECONDS_PER_MILLISECOND * Self::MILLISECONDS_PER_SECOND;
    pub const MICROSECONDS_PER_MINUTE: i64 = Self::MICROSECONDS_PER_SECOND * 60;
    pub const MICROSECONDS_PER_HOUR: i64 = Self::MICROSECONDS_PER_MINUTE * 60;
    pub const MICROSECONDS_PER_DAY: i64 = Self::MICROSECONDS_PER_HOUR * Self::HOURS_PER_DAY;
    pub const NANOSECONDS_PER_MICROSECOND: i64 = 1000;
    pub const NANOSECONDS_PER_SECOND: i64 = Self::NANOSECONDS_PER_MICROSECOND * Self::MICROSECONDS_PER_SECOND;
}

/// Represents a duration of time, internally represented in microseconds.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeDelta {
    delta_us: i64,
}

impl TimeDelta {
    pub const ZERO: Self = Self { delta_us: 0 };
    pub const MAX: Self = Self { delta_us: i64::MAX };
    pub const MIN: Self = Self { delta_us: i64::MIN };

    #[inline]
    pub const fn from_microseconds(us: i64) -> Self {
        Self { delta_us: us }
    }

    #[inline]
    pub const fn from_milliseconds(ms: i64) -> Self {
        Self {
            delta_us: ms.saturating_mul(TimeConstants::MICROSECONDS_PER_MILLISECOND),
        }
    }

    #[inline]
    pub const fn from_seconds(s: i64) -> Self {
        Self {
            delta_us: s.saturating_mul(TimeConstants::MICROSECONDS_PER_SECOND),
        }
    }

    #[inline]
    pub const fn from_minutes(m: i32) -> Self {
        Self {
            delta_us: (m as i64).saturating_mul(TimeConstants::MICROSECONDS_PER_MINUTE),
        }
    }

    #[inline]
    pub const fn from_hours(h: i32) -> Self {
        Self {
            delta_us: (h as i64).saturating_mul(TimeConstants::MICROSECONDS_PER_HOUR),
        }
    }

    #[inline]
    pub const fn from_days(d: i32) -> Self {
        Self {
            delta_us: (d as i64).saturating_mul(TimeConstants::MICROSECONDS_PER_DAY),
        }
    }

    #[inline]
    pub const fn from_nanoseconds(ns: i64) -> Self {
        Self {
            delta_us: ns / TimeConstants::NANOSECONDS_PER_MICROSECOND,
        }
    }

    #[inline]
    pub const fn in_microseconds(&self) -> i64 {
        self.delta_us
    }

    #[inline]
    pub const fn in_milliseconds(&self) -> i64 {
        self.delta_us / TimeConstants::MICROSECONDS_PER_MILLISECOND
    }

    #[inline]
    pub const fn in_seconds(&self) -> i64 {
        self.delta_us / TimeConstants::MICROSECONDS_PER_SECOND
    }

    #[inline]
    pub const fn in_nanoseconds(&self) -> i64 {
        self.delta_us.saturating_mul(TimeConstants::NANOSECONDS_PER_MICROSECOND)
    }

    #[inline]
    pub const fn is_zero(&self) -> bool {
        self.delta_us == 0
    }

    #[inline]
    pub const fn is_negative(&self) -> bool {
        self.delta_us < 0
    }
}

impl std::ops::Add for TimeDelta {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            delta_us: self.delta_us.saturating_add(rhs.delta_us),
        }
    }
}

impl std::ops::Sub for TimeDelta {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            delta_us: self.delta_us.saturating_sub(rhs.delta_us),
        }
    }
}

/// Represents a monotonic point in time.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeTicks {
    micros_since_origin: i64,
}

static TICK_ORIGIN: OnceLock<Instant> = OnceLock::new();

impl TimeTicks {
    pub fn now() -> Self {
        let origin = TICK_ORIGIN.get_or_init(Instant::now);
        let elapsed = origin.elapsed();
        Self {
            micros_since_origin: elapsed.as_micros() as i64,
        }
    }

    #[inline]
    pub const fn is_high_resolution() -> bool {
        true
    }

    #[inline]
    pub const fn in_microseconds(&self) -> i64 {
        self.micros_since_origin
    }

    #[inline]
    pub fn since(&self, earlier: Self) -> TimeDelta {
        TimeDelta::from_microseconds(self.micros_since_origin.saturating_sub(earlier.micros_since_origin))
    }
}

/// Represents wall-clock time (UTC).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    us_since_epoch: i64,
}

impl Time {
    pub fn now() -> Self {
        let now = SystemTime::now();
        let us = match now.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_micros() as i64,
            Err(e) => -(e.duration().as_micros() as i64),
        };
        Self { us_since_epoch: us }
    }

    #[inline]
    pub const fn from_unix_timestamp_micros(us: i64) -> Self {
        Self { us_since_epoch: us }
    }

    #[inline]
    pub const fn to_unix_timestamp_micros(&self) -> i64 {
        self.us_since_epoch
    }
}
