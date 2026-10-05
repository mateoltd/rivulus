//! Snowflake + misc utils.
/// Milliseconds threshold.
#[must_use]
#[allow(clippy::cast_possible_wrap)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
pub fn snowflake_timestamp(id: u64) -> i64 {
    const EPOCH: i64 = 1_420_070_400_000;
    ((id >> 22) as i64) + EPOCH
}
