//! The line that greets you on a new chat. It changes: each new chat
//! picks another, every few minutes while it's showing it moves on, and
//! the time of day has its own lines.

/// The part of the day an hour (0..24, local) falls in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DayPart {
    Morning,
    Afternoon,
    Evening,
    Night,
}

pub fn day_part(hour: u32) -> DayPart {
    match hour {
        5..=11 => DayPart::Morning,
        12..=16 => DayPart::Afternoon,
        17..=21 => DayPart::Evening,
        _ => DayPart::Night,
    }
}

/// Lines for any time. `{}` is your name.
const ANYTIME: &[&str] = &[
    "Welcome back, {}",
    "{} returns",
    "What are we forging, {}?",
    "Back at the anvil, {}",
    "The forge is lit, {}",
    "Ready when you are, {}",
    "Let's build something, {}",
    "Good to see you, {}",
];

fn of_day(part: DayPart) -> &'static [&'static str] {
    match part {
        DayPart::Morning => &[
            "Good morning, {}",
            "Morning, {}. Fresh start?",
            "Early fire, {}",
        ],
        DayPart::Afternoon => &["Good afternoon, {}", "Afternoon, {}. What's next?"],
        DayPart::Evening => &["Good evening, {}", "Evening shift, {}"],
        DayPart::Night => &[
            "Burning the midnight oil, {}?",
            "Late at the forge, {}",
            "Quiet hours, {}",
        ],
    }
}

/// A small deterministic mix, so a seed always picks the same line.
fn mix(x: u64) -> u64 {
    let mut x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// The greeting for `name` at `hour`, picked by `seed` from the time of
/// day's lines and the ones for any time.
pub fn pick(name: &str, hour: u32, seed: u64) -> String {
    let name = name.trim();
    if name.is_empty() {
        return "Welcome to Forge".into();
    }
    let day = of_day(day_part(hour));
    let all = day.len() + ANYTIME.len();
    let i = (mix(seed) % all as u64) as usize;
    let line = if i < day.len() {
        day[i]
    } else {
        ANYTIME[i - day.len()]
    };
    line.replace("{}", name)
}

/// The local hour now.
pub fn hour_now() -> u32 {
    use chrono::Timelike;
    chrono::Local::now().hour()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_day_is_split_in_four() {
        assert_eq!(
            (
                day_part(5),
                day_part(11),
                day_part(12),
                day_part(17),
                day_part(22),
                day_part(3)
            ),
            (
                DayPart::Morning,
                DayPart::Morning,
                DayPart::Afternoon,
                DayPart::Evening,
                DayPart::Night,
                DayPart::Night
            )
        );
    }

    #[test]
    fn it_varies_and_always_names_you() {
        let lines: std::collections::HashSet<String> =
            (0..64).map(|s| pick("Amine", 9, s)).collect();
        assert!(lines.len() >= 6, "{lines:?}");
        assert!(lines.iter().all(|l| l.contains("Amine")));
        // Night lines never show in the morning.
        assert!(!lines.iter().any(|l| l.contains("midnight")));
        assert_eq!(pick("Amine", 9, 7), pick("Amine", 9, 7));
        assert_eq!(pick("  ", 9, 1), "Welcome to Forge");
    }
}
