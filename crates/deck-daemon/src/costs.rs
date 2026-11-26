//! What today cost, from the running totals of the sessions (`cost-state`).
//!
//! A session reports its total so far, not what a day added. The ledger keeps
//! per session where the day began and where it stands, so "today" is the sum
//! of the differences. A session first seen today but begun before counts only
//! from then on: Deck shows too little rather than too much. All of it is an
//! estimate of Claude Code.

use deck_proto::fsperm;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use chrono::{Local, NaiveTime, TimeZone};
use deck_usage::CostState;
use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "costs.json";
/// Sessions not heard of for this many days leave the ledger.
const KEEP_DAYS: i64 = 7;

/// The calendar day of the user and when it began.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Day {
    /// Days since the epoch of the local calendar: grows by one per day.
    pub number: i64,
    pub start_ms: u64,
}

/// The local day of a moment.
pub fn local_day(now_ms: u64) -> Day {
    let now = Local
        .timestamp_millis_opt(i64::try_from(now_ms).unwrap_or(0))
        .earliest()
        .unwrap_or_else(Local::now);
    let date = now.date_naive();
    // Midnight does not exist on a few days of some time zones.
    let start = date
        .and_time(NaiveTime::MIN)
        .and_local_timezone(Local)
        .earliest()
        .map(|t| t.timestamp_millis())
        .unwrap_or_else(|| now.timestamp_millis());
    Day {
        number: date
            .signed_duration_since(chrono::NaiveDate::default())
            .num_days(),
        start_ms: u64::try_from(start).unwrap_or(0),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    /// Total when the day began (or when first seen).
    base: u64,
    /// Latest total.
    last: u64,
    /// Spent today before the total started again (a reset of the session).
    carried: u64,
    seen_day: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    day: i64,
    sessions: BTreeMap<String, Entry>,
}

impl Ledger {
    /// A new day: what was spent before stays before.
    pub fn roll(&mut self, today: &Day) -> bool {
        if self.day == today.number {
            return false;
        }
        self.day = today.number;
        self.sessions
            .retain(|_, e| today.number - e.seen_day <= KEEP_DAYS);
        for e in self.sessions.values_mut() {
            e.base = e.last;
            e.carried = 0;
        }
        true
    }

    pub fn record(&mut self, session: &str, cost: CostState, today: &Day) {
        self.roll(today);
        let total = cost.total_micro_usd;
        let e = match self.sessions.get_mut(session) {
            Some(e) => {
                if total < e.last {
                    e.carried = e.carried.saturating_add(e.last.saturating_sub(e.base));
                    e.base = 0;
                }
                e
            }
            None => {
                let began_today = cost.started_ms.is_some_and(|s| s >= today.start_ms);
                self.sessions.entry(session.to_owned()).or_insert(Entry {
                    base: if began_today { 0 } else { total },
                    ..Entry::default()
                })
            }
        };
        e.last = total;
        e.seen_day = today.number;
    }

    /// What today cost so far.
    pub fn today(&self) -> u64 {
        self.sessions.values().fold(0u64, |sum, e| {
            sum.saturating_add(e.carried)
                .saturating_add(e.last.saturating_sub(e.base))
        })
    }
}

pub fn load(path: &Path) -> Ledger {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Private from the first byte, replaced in one step.
pub fn save(path: &Path, ledger: &Ledger) -> std::io::Result<()> {
    let json = serde_json::to_vec(ledger).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut f = fsperm::create_mode(
        std::fs::OpenOptions::new().write(true).create_new(true),
        fsperm::PRIVATE_FILE,
    )
    .open(&tmp)?;
    fsperm::set_mode(&f, fsperm::PRIVATE_FILE)?;
    f.write_all(&json)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(n: i64) -> Day {
        Day {
            number: n,
            start_ms: (n as u64) * 86_400_000,
        }
    }

    fn cost(total: u64, started: u64) -> CostState {
        CostState {
            total_micro_usd: total,
            started_ms: Some(started),
        }
    }

    #[test]
    fn a_session_of_today_counts_in_full() {
        let d = day(100);
        let mut l = Ledger::default();
        l.record("a", cost(500, d.start_ms + 5), &d);
        l.record("a", cost(900, d.start_ms + 5), &d);
        l.record("b", cost(100, d.start_ms + 9), &d);
        assert_eq!(l.today(), 1000);
    }

    #[test]
    fn an_older_session_counts_only_what_it_adds_from_when_seen() {
        let d = day(100);
        let mut l = Ledger::default();
        l.record("a", cost(5000, 1), &d);
        assert_eq!(l.today(), 0);
        l.record("a", cost(5600, 1), &d);
        assert_eq!(l.today(), 600);
    }

    #[test]
    fn midnight_starts_at_zero_and_keeps_the_sessions_apart() {
        let mut l = Ledger::default();
        let d = day(100);
        l.record("a", cost(700, d.start_ms + 1), &d);
        let next = day(101);
        assert!(l.roll(&next));
        assert!(!l.roll(&next));
        assert_eq!(l.today(), 0);
        l.record("a", cost(900, d.start_ms + 1), &next);
        assert_eq!(l.today(), 200);
    }

    #[test]
    fn a_total_that_starts_again_loses_nothing() {
        let d = day(100);
        let mut l = Ledger::default();
        l.record("a", cost(800, d.start_ms + 1), &d);
        l.record("a", cost(100, d.start_ms + 2), &d);
        assert_eq!(l.today(), 900);
    }

    #[test]
    fn old_sessions_leave_the_ledger() {
        let mut l = Ledger::default();
        l.record("a", cost(1, 1), &day(100));
        l.roll(&day(100 + KEEP_DAYS + 1));
        assert!(l.sessions.is_empty());
    }

    #[test]
    fn the_ledger_survives_a_restart_privately() {
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path().join(FILE_NAME);
        let mut l = Ledger::default();
        l.record("a", cost(5, 1), &day(1));
        save(&p, &l).expect("save");
        assert_eq!(load(&p), l);
        assert_eq!(
            fsperm::mode(&std::fs::metadata(&p).expect("meta")).unwrap_or(0o600),
            0o600
        );
        assert_eq!(load(&dir.path().join("none")), Ledger::default());
    }

    #[test]
    fn the_local_day_begins_before_now() {
        let now = 1_800_000_000_000;
        let d = local_day(now);
        assert!(d.start_ms <= now && now - d.start_ms < 26 * 3_600_000);
        assert_eq!(local_day(now + 48 * 3_600_000).number, d.number + 2);
    }
}
