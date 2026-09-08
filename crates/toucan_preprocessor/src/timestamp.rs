use std::fmt;
use std::str::FromStr;

/// A fixed UTC timestamp for the C `__DATE__` and `__TIME__` macros.
///
/// The library never reads a clock, environment variable, locale, or time zone.
/// Values span the Unix epoch through 9999-12-31 23:59:59, excluding leap seconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreprocessingTimestamp {
    unix_seconds: u64,
    date: [u8; 13],
    time: [u8; 10],
}

impl PreprocessingTimestamp {
    /// The deterministic default for library callers: 1970-01-01 00:00:00 UTC.
    pub const UNIX_EPOCH: Self = Self {
        unix_seconds: 0,
        date: *b"\"Jan  1 1970\"",
        time: *b"\"00:00:00\"",
    };

    /// The last second whose date has a four-digit year.
    pub const MAX_UNIX_SECONDS: u64 = 253_402_300_799;

    /// Validate seconds since the Unix epoch and prepare both C string literals.
    pub fn from_unix_seconds(unix_seconds: u64) -> Result<Self, TimestampError> {
        if unix_seconds > Self::MAX_UNIX_SECONDS {
            return Err(TimestampError);
        }
        let absolute_day = unix_seconds / 86_400 + days_before_year(1970);
        // Find the Gregorian year in a fixed number of steps, even at the
        // maximum timestamp. Formatting is performed once when configured.
        let (mut year, mut upper) = (1970, 10_000);
        while upper - year > 1 {
            let middle = (year + upper) / 2;
            if days_before_year(middle) <= absolute_day {
                year = middle;
            } else {
                upper = middle;
            }
        }
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let month_days = [
            31,
            28 + u64::from(leap),
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let mut day = absolute_day - days_before_year(year);
        let mut month = 0;
        while day >= month_days[month] {
            day -= month_days[month];
            month += 1;
        }
        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let date = format!("\"{} {:2} {year:04}\"", months[month], day + 1);
        let seconds = unix_seconds % 86_400;
        let time = format!(
            "\"{:02}:{:02}:{:02}\"",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        );
        Ok(Self {
            unix_seconds,
            date: date
                .as_bytes()
                .try_into()
                .expect("validated four-digit year"),
            time: time.as_bytes().try_into().expect("validated time of day"),
        })
    }

    /// Return the configured Unix timestamp, independent of local time zones.
    pub const fn unix_seconds(self) -> u64 {
        self.unix_seconds
    }

    pub(crate) fn date_literal(&self) -> &str {
        std::str::from_utf8(&self.date).expect("timestamp is ASCII")
    }

    pub(crate) fn time_literal(&self) -> &str {
        std::str::from_utf8(&self.time).expect("timestamp is ASCII")
    }
}

impl Default for PreprocessingTimestamp {
    fn default() -> Self {
        Self::UNIX_EPOCH
    }
}

impl FromStr for PreprocessingTimestamp {
    type Err = TimestampError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(TimestampError);
        }
        Self::from_unix_seconds(value.parse().map_err(|_| TimestampError)?)
    }
}

/// A timestamp outside the supported range, or not an unsigned decimal integer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimestampError;

impl fmt::Display for TimestampError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .write_str("timestamp must be an ASCII decimal integer from 0 through 253402300799")
    }
}

impl std::error::Error for TimestampError {}

fn days_before_year(year: u64) -> u64 {
    let previous = year - 1;
    previous * 365 + previous / 4 - previous / 100 + previous / 400
}
