// SPDX-License-Identifier: GPL-2.0-or-later
//! A C2H read the FPGA does not finish (`seq::DMA_TIMEOUT`, xdma.ko's `c2h_timeout`, 1 s since `oos-fpga-up` sets
//! it): counted, logged at most every `LOG_PERIOD_US`, and the acquisition goes on. The first one is taken again at
//! once: the next pass configures anew and its arm starts with the stop sequence. Timeouts in a row wait longer
//! before the next arm, from 1 s doubling to `BACKOFF_MAX_US`, while the loop still serves requests; a read that
//! succeeds ends the series. The idea comes from fumingcaps' fork yottabytesolutions/oos (release v0.2.2).

/// Shortest time between two log lines of timeouts.
const LOG_PERIOD_US: u64 = 5_000_000;
/// Wait before the next arm after the second timeout in a row; doubled for each further one.
const BACKOFF_FIRST_US: u64 = 1_000_000;
/// Longest wait before the next arm.
const BACKOFF_MAX_US: u64 = 30_000_000;

/// Every how many batches a read is made to stall (`DmaBuf::stall_next_read`): `OOS_DMA_STALL=<n>` in the environment,
/// for a test of this path on a unit. None without it.
fn stall_every() -> Option<u64> {
    static N: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *N.get_or_init(|| std::env::var("OOS_DMA_STALL").ok().and_then(|v| v.trim().parse().ok()).filter(|&n| n > 0))
}

/// Timeouts of the C2H read of the live loop, and the wait before the next arm.
#[derive(Default)]
pub(super) struct DmaFault {
    /// timeouts in a row (none since the last read that succeeded)
    in_row: u32,
    /// no arm before this time (the loop's clock, us)
    until_us: u64,
    /// when the last line was logged
    logged_us: Option<u64>,
    /// timeouts not yet in a log line
    unlogged: u64,
    /// batches since the start, for `stall_every`
    batches: u64,
}

impl DmaFault {
    /// A timeout at `now_us`: the line to log, when one is due (with the timeouts since the last line).
    pub(super) fn timeout(&mut self, now_us: u64, error: &str) -> Option<String> {
        self.in_row += 1;
        self.unlogged += 1;
        let wait = backoff_us(self.in_row);
        self.until_us = now_us + wait;
        crate::board::pipe::Counters::add(&crate::board::pipe::counters().dma_timeouts, 1);
        if self.logged_us.is_some_and(|t| now_us.saturating_sub(t) < LOG_PERIOD_US) {
            return None;
        }
        let line = format!(
            "acquire: {} ({} since the last line, {} in a row; next arm in {} ms)",
            error,
            self.unlogged,
            self.in_row,
            wait / 1000
        );
        (self.logged_us, self.unlogged) = (Some(now_us), 0);
        Some(line)
    }

    /// Whether the read of the batch about to be armed is made to stall (`stall_every`).
    pub(super) fn stall_due(&mut self) -> bool {
        stall_every().is_some_and(|n| {
            self.batches += 1;
            self.batches.is_multiple_of(n)
        })
    }

    /// Whether the next arm waits at `now_us`.
    pub(super) fn waiting(&self, now_us: u64) -> bool {
        now_us < self.until_us
    }

    /// A read succeeded: the series ends.
    pub(super) fn read_ok(&mut self) {
        (self.in_row, self.until_us) = (0, 0);
    }
}

/// The wait before the next arm after the `n`th timeout in a row: none after the first.
fn backoff_us(n: u32) -> u64 {
    match n {
        0 | 1 => 0,
        n => BACKOFF_FIRST_US.saturating_mul(1 << (n - 2).min(16)).min(BACKOFF_MAX_US),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeouts_in_a_row_wait_longer_and_a_good_read_ends_the_series() {
        let mut f = DmaFault::default();
        let s = 1_000_000;
        // the first one: logged, taken again at once
        assert!(f.timeout(10 * s, "c2h DMA timeout").is_some());
        assert!(!f.waiting(10 * s));
        // the second: 1 s, not logged within 5 s of the first
        assert!(f.timeout(11 * s, "c2h DMA timeout").is_none());
        assert!(f.waiting(11 * s) && !f.waiting(12 * s));
        // then 2 s, 4 s, ... up to 30 s; the line counts what it did not log
        let line = f.timeout(16 * s, "c2h DMA timeout").unwrap();
        assert!(line.contains("2 since the last line, 3 in a row; next arm in 2000 ms"), "{}", line);
        for _ in 0..20 {
            f.timeout(100 * s, "x");
        }
        assert!(f.waiting(129 * s) && !f.waiting(130 * s), "at most 30 s");
        f.read_ok();
        assert!(!f.waiting(100 * s));
        f.timeout(200 * s, "x");
        assert!(!f.waiting(200 * s), "a new series starts at once");
    }
}
