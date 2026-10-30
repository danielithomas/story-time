//! The writing log: word-count snapshots in `.story-time/history.csv`, and what they add up to.
//!
//! A snapshot is taken whenever a command measures the vault, and only for projects whose count
//! changed. Words written on a day are the net change between snapshots, so words written on a
//! day when nothing was measured count towards the next day that is. The file lives in the vault,
//! so it is backed up and synced with everything else.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{NaiveDate, NaiveDateTime};

const HEADER: &str = "timestamp,project,words";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub at: NaiveDateTime,
    pub project: String,
    pub words: u64,
}

#[derive(Debug, Default)]
pub struct History {
    snapshots: Vec<Snapshot>,
}

impl History {
    /// Loads the log; a missing file is an empty log, and unreadable lines are skipped.
    pub fn load(path: &Path) -> Result<History> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(History::default()),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        let mut snapshots: Vec<Snapshot> = text.lines().filter_map(parse_line).collect();
        snapshots.sort_by_key(|snapshot| snapshot.at);
        Ok(History { snapshots })
    }

    pub fn latest(&self, project: &str) -> Option<u64> {
        self.snapshots
            .iter()
            .rev()
            .find(|s| s.project == project)
            .map(|s| s.words)
    }

    /// Appends a snapshot for each project whose count differs from its last one, and returns
    /// how many were written.
    pub fn record(
        &mut self,
        path: &Path,
        at: NaiveDateTime,
        counts: &[(String, u64)],
    ) -> Result<usize> {
        let fresh = self.changes(at, counts);
        if fresh.is_empty() {
            return Ok(0);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let is_new = !path.exists();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        let mut lines = String::new();
        if is_new {
            lines.push_str(HEADER);
            lines.push('\n');
        }
        for snapshot in &fresh {
            lines.push_str(&format!(
                "{},{},{}\n",
                snapshot.at.format(TIME_FORMAT),
                snapshot.project,
                snapshot.words
            ));
        }
        file.write_all(lines.as_bytes())
            .with_context(|| format!("writing {}", path.display()))?;
        let written = fresh.len();
        self.snapshots.extend(fresh);
        Ok(written)
    }

    /// The log as it would be with these counts recorded at `at`, without writing anything.
    /// Lets a front end show today's words live while snapshots are written less often.
    pub fn preview(&self, at: NaiveDateTime, counts: &[(String, u64)]) -> History {
        let mut snapshots = self.snapshots.clone();
        snapshots.extend(self.changes(at, counts));
        History { snapshots }
    }

    fn changes(&self, at: NaiveDateTime, counts: &[(String, u64)]) -> Vec<Snapshot> {
        counts
            .iter()
            .filter(|(project, words)| self.latest(project) != Some(*words))
            .map(|(project, words)| Snapshot {
                at,
                project: project.clone(),
                words: *words,
            })
            .collect()
    }

    /// Net words written per day, for one project or all. A project's first snapshot is its
    /// starting point, not words written.
    pub fn daily(&self, project: Option<&str>) -> BTreeMap<NaiveDate, i64> {
        let mut last: HashMap<&str, u64> = HashMap::new();
        let mut days = BTreeMap::new();
        for snapshot in &self.snapshots {
            let previous = last.insert(&snapshot.project, snapshot.words);
            if project.is_some_and(|p| p != snapshot.project) {
                continue;
            }
            let change = previous.map_or(0, |previous| snapshot.words as i64 - previous as i64);
            *days.entry(snapshot.at.date()).or_insert(0) += change;
        }
        days
    }

    pub fn written_on(&self, day: NaiveDate, project: Option<&str>) -> i64 {
        self.daily(project).get(&day).copied().unwrap_or(0)
    }

    /// Consecutive days with words written, ending today, or yesterday if today has none yet.
    pub fn streak(&self, today: NaiveDate) -> u32 {
        let days = self.daily(None);
        let wrote = |day: NaiveDate| days.get(&day).is_some_and(|words| *words > 0);
        let mut day = if wrote(today) {
            today
        } else {
            today.pred_opt().unwrap_or(today)
        };
        let mut streak = 0;
        while wrote(day) {
            streak += 1;
            match day.pred_opt() {
                Some(previous) => day = previous,
                None => break,
            }
        }
        streak
    }
}

fn parse_line(line: &str) -> Option<Snapshot> {
    let mut fields = line.trim().splitn(3, ',');
    let at = NaiveDateTime::parse_from_str(fields.next()?, TIME_FORMAT).ok()?;
    let project = fields.next()?.to_string();
    let words = fields.next()?.parse().ok()?;
    Some(Snapshot { at, project, words })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn at(day: u32, hour: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, day)
            .unwrap()
            .and_hms_opt(hour, 0, 0)
            .unwrap()
    }

    fn counts(pairs: &[(&str, u64)]) -> Vec<(String, u64)> {
        pairs.iter().map(|(p, w)| (p.to_string(), *w)).collect()
    }

    #[test]
    fn records_only_changes_and_round_trips() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".story-time/history.csv");
        let mut history = History::load(&path).unwrap();
        assert_eq!(
            history
                .record(&path, at(28, 9), &counts(&[("alpha", 1000), ("beta", 0)]))
                .unwrap(),
            2
        );
        assert_eq!(
            history
                .record(&path, at(28, 21), &counts(&[("alpha", 1500), ("beta", 0)]))
                .unwrap(),
            1
        );
        assert_eq!(
            history
                .record(&path, at(28, 22), &counts(&[("alpha", 1500)]))
                .unwrap(),
            0
        );
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "timestamp,project,words\n2026-10-28T09:00:00,alpha,1000\n2026-10-28T09:00:00,beta,0\n2026-10-28T21:00:00,alpha,1500\n"
        );
        let reloaded = History::load(&path).unwrap();
        assert_eq!(reloaded.latest("alpha"), Some(1500));
        assert_eq!(reloaded.written_on(at(28, 0).date(), None), 500);
    }

    #[test]
    fn daily_totals_and_streaks() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.csv");
        let mut history = History::default();
        history
            .record(&path, at(25, 9), &counts(&[("alpha", 1000)]))
            .unwrap();
        history
            .record(&path, at(26, 9), &counts(&[("alpha", 1200)]))
            .unwrap();
        history
            .record(&path, at(27, 9), &counts(&[("alpha", 1100), ("beta", 300)]))
            .unwrap();
        history
            .record(&path, at(28, 9), &counts(&[("alpha", 1400), ("beta", 500)]))
            .unwrap();
        history
            .record(&path, at(29, 9), &counts(&[("beta", 900)]))
            .unwrap();

        let daily = history.daily(None);
        assert_eq!(
            daily.get(&at(25, 0).date()),
            Some(&0),
            "the first snapshot is a baseline"
        );
        assert_eq!(daily.get(&at(26, 0).date()), Some(&200));
        assert_eq!(
            daily.get(&at(27, 0).date()),
            Some(&-100),
            "editing can remove words"
        );
        assert_eq!(daily.get(&at(28, 0).date()), Some(&500));
        assert_eq!(history.written_on(at(28, 0).date(), Some("beta")), 200);

        assert_eq!(
            history.streak(at(29, 0).date()),
            2,
            "28th and 29th; the 27th was net negative"
        );
        assert_eq!(
            history.streak(at(30, 0).date()),
            2,
            "a streak survives until the day is over"
        );
        assert_eq!(history.streak(at(31, 0).date()), 0);

        let preview = history.preview(at(29, 18), &counts(&[("beta", 1000)]));
        assert_eq!(preview.written_on(at(29, 0).date(), None), 500);
        assert_eq!(
            history.written_on(at(29, 0).date(), None),
            400,
            "preview changes nothing"
        );
        assert!(!fs::read_to_string(&path).unwrap().contains("beta,1000"));
    }
}
