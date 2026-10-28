//! Routines (Open Design's `routines.ts` and automations): a request Forge
//! runs on a schedule, as a new chat in its project, with its design setup:
//! "every Monday at 9, refresh the metrics deck from data.csv". They run
//! while Forge is open; one missed while it was closed runs once when it
//! opens. Kept in `routines.json` in Forge's config folder.

use std::path::PathBuf;

use chrono::{DateTime, Datelike, Local, NaiveTime, TimeZone, Weekday};
use serde::{Deserialize, Serialize};

/// How often.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "every", rename_all = "lowercase")]
pub enum Every {
    /// At this minute past each hour.
    Hour { minute: u32 },
    /// Each day at this time (`"09:00"`).
    Day { at: String },
    /// Each week on this day (`"mon"` … `"sun"`) at this time.
    Week { day: String, at: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub every: Every,
    /// The project it runs in; None: the one open when it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<PathBuf>,
    /// Its design setup: it runs as a design chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<crate::design::prompt::Setup>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// When it last ran (Unix seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<i64>,
}

fn yes() -> bool {
    true
}

fn time(at: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(at.trim(), "%H:%M").ok()
}

fn weekday(day: &str) -> Option<Weekday> {
    day.trim().to_lowercase().get(..3)?.parse().ok()
}

impl Every {
    /// Read "hourly at 15", "daily at 09:00", "weekly on mon at 09:00" (and
    /// "every day at 9:30", "mondays at 9:00").
    pub fn parse(s: &str) -> Option<Every> {
        let s = s.trim().to_lowercase();
        let at = s
            .split(" at ")
            .nth(1)
            .map(|t| t.trim().to_string())
            .map(|t| {
                if t.contains(':') {
                    t
                } else {
                    format!("{t}:00")
                }
            });
        let at = at.and_then(|t| time(&t).map(|n| n.format("%H:%M").to_string()));
        if s.starts_with("hourly") || s.starts_with("every hour") {
            let minute = s
                .split(" at ")
                .nth(1)
                .and_then(|m| m.trim().trim_start_matches(':').parse().ok())
                .unwrap_or(0)
                .min(59);
            return Some(Every::Hour { minute });
        }
        if s.starts_with("daily") || s.starts_with("every day") || s.starts_with("each day") {
            return Some(Every::Day { at: at? });
        }
        let day = s
            .split_whitespace()
            .find_map(|w| weekday(w.trim_end_matches('s')))?;
        Some(Every::Week {
            day: format!("{day}").to_lowercase(),
            at: at?,
        })
    }

    pub fn say(&self) -> String {
        match self {
            Every::Hour { minute } => format!("hourly at :{minute:02}"),
            Every::Day { at } => format!("daily at {at}"),
            Every::Week { day, at } => format!("weekly on {day} at {at}"),
        }
    }

    /// The last time it was due at or before `now`.
    fn last_due(&self, now: DateTime<Local>) -> Option<DateTime<Local>> {
        let at_day = |date: chrono::NaiveDate, t: NaiveTime| {
            Local.from_local_datetime(&date.and_time(t)).earliest()
        };
        match self {
            Every::Hour { minute } => {
                let t = now
                    .date_naive()
                    .and_hms_opt(now.hour_of_day(), *minute, 0)?;
                let due = Local.from_local_datetime(&t).earliest()?;
                Some(if due <= now {
                    due
                } else {
                    due - chrono::Duration::hours(1)
                })
            }
            Every::Day { at } => {
                let t = time(at)?;
                let today = at_day(now.date_naive(), t)?;
                Some(if today <= now {
                    today
                } else {
                    at_day(now.date_naive().pred_opt()?, t)?
                })
            }
            Every::Week { day, at } => {
                let (wd, t) = (weekday(day)?, time(at)?);
                let mut date = now.date_naive();
                for _ in 0..8 {
                    if date.weekday() == wd {
                        if let Some(due) = at_day(date, t).filter(|d| *d <= now) {
                            return Some(due);
                        }
                    }
                    date = date.pred_opt()?;
                }
                None
            }
        }
    }
}

trait Hour {
    fn hour_of_day(&self) -> u32;
}

impl Hour for DateTime<Local> {
    fn hour_of_day(&self) -> u32 {
        chrono::Timelike::hour(self)
    }
}

impl Routine {
    /// It should run now: due since it last ran. A new routine's `last_run`
    /// is when it was made, so it waits for its first time.
    pub fn due(&self, now: DateTime<Local>) -> bool {
        if !self.enabled {
            return false;
        }
        let Some(due) = self.every.last_due(now) else {
            return false;
        };
        due.timestamp() > self.last_run.unwrap_or(i64::MIN)
    }
}

pub fn file() -> Option<PathBuf> {
    crate::profile::dir().map(|d| d.join("routines.json"))
}

pub fn load() -> Vec<Routine> {
    file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(list: &[Routine]) -> std::io::Result<()> {
    let Some(f) = file() else { return Ok(()) };
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = f.with_extension("json.part");
    std::fs::write(
        &tmp,
        serde_json::to_string_pretty(list).unwrap_or_else(|_| "[]".into()),
    )?;
    std::fs::rename(tmp, f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Local> {
        Local
            .from_local_datetime(
                &chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap(),
            )
            .earliest()
            .unwrap()
    }

    #[test]
    fn schedules_read_as_people_write_them() {
        assert_eq!(
            Every::parse("daily at 9"),
            Some(Every::Day { at: "09:00".into() })
        );
        assert_eq!(
            Every::parse("every day at 18:30"),
            Some(Every::Day { at: "18:30".into() })
        );
        assert_eq!(
            Every::parse("weekly on Monday at 09:15"),
            Some(Every::Week {
                day: "mon".into(),
                at: "09:15".into()
            })
        );
        assert_eq!(
            Every::parse("fridays at 17:00"),
            Some(Every::Week {
                day: "fri".into(),
                at: "17:00".into()
            })
        );
        assert_eq!(
            Every::parse("hourly at 15"),
            Some(Every::Hour { minute: 15 })
        );
        assert_eq!(Every::parse("sometimes"), None);
        assert_eq!(Every::parse("daily at 25:00"), None);
    }

    #[test]
    fn a_routine_runs_once_per_time_it_is_due() {
        let mut r = Routine {
            id: "r".into(),
            name: "Metrics deck".into(),
            prompt: "Refresh the deck".into(),
            every: Every::Week {
                day: "mon".into(),
                at: "09:00".into(),
            },
            project: None,
            design: None,
            enabled: true,
            // Made on Sunday 2026-10-25: not due until Monday 9:00.
            last_run: Some(at("2026-10-25 12:00").timestamp()),
        };
        assert!(!r.due(at("2026-10-26 08:59")));
        assert!(r.due(at("2026-10-26 09:00")));
        r.last_run = Some(at("2026-10-26 09:00").timestamp());
        assert!(!r.due(at("2026-10-26 18:00")));
        // Missed while Forge was closed: runs once when it opens.
        assert!(r.due(at("2026-11-04 10:00")));
        r.enabled = false;
        assert!(!r.due(at("2026-11-04 10:00")));
        let made = Some(at("2026-10-25 12:00").timestamp());
        let daily = Routine {
            every: Every::Day { at: "09:00".into() },
            enabled: true,
            last_run: made,
            ..r.clone()
        };
        assert!(
            !daily.due(at("2026-10-25 13:00")),
            "already past 9 when made: tomorrow"
        );
        assert!(daily.due(at("2026-10-26 09:01")));
        let hourly = Routine {
            every: Every::Hour { minute: 30 },
            ..daily.clone()
        };
        assert!(hourly.due(at("2026-10-25 12:31")));
        assert!(!hourly.due(at("2026-10-25 12:29")));
        // Round trip.
        let json = serde_json::to_string(&vec![r.clone()]).unwrap();
        assert_eq!(
            serde_json::from_str::<Vec<Routine>>(&json).unwrap(),
            vec![r]
        );
    }
}
