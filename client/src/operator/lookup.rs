//! Only an explicit missing-path reply permits another lookup attempt.
use system_api::operator::Fail;
pub(super) enum Failure {
    Missing,
    Stop(Fail),
}
pub(super) fn until<T>(
    mut call: impl FnMut() -> Result<T, Failure>,
    mut again: impl FnMut(usize) -> bool,
) -> Result<T, Fail> {
    let mut backoff = 10;
    loop {
        match call() {
            Ok(value) => return Ok(value),
            Err(Failure::Missing) if again(backoff) => {
                backoff = (backoff * 2).min(100);
            }
            Err(Failure::Missing) => return Err(Fail::Unknown),
            Err(Failure::Stop(fail)) => return Err(fail),
        }
    }
}

/// Retry only after an explicit missing-path answer. Each attempt samples the
/// remaining budget exactly once: the same value is used for the guard and the
/// call. Transport failures from calls that still run keep their original
/// `Stop`; no second clock read can turn an admitted attempt into `POLL`.
pub(super) fn until_after_unknown<T, W>(
    retry_missing: bool,
    mut budget: impl FnMut() -> (W, bool),
    mut call: impl FnMut(W) -> Result<T, Failure>,
    again: impl FnMut(usize) -> bool,
) -> Result<T, Fail> {
    let mut explicit_missing = false;
    until(
        || {
            let (remaining, at_poll) = budget();
            if retry_missing && explicit_missing && at_poll {
                return Err(Failure::Missing);
            }
            match call(remaining) {
                Err(Failure::Missing) => {
                    explicit_missing = true;
                    Err(Failure::Missing)
                }
                other => other,
            }
        },
        again,
    )
}
