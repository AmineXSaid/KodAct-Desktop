//! KodAct CLI's nudges and its reasoning carry-over, as switches in
//! Settings. Each is a key in KodAct's own `settings.json` (`nudges.<name>`,
//! `carryReasoning`), which is what KodAct CLI reads when a chat's process
//! starts (`backend::launch` turns `nudges` into `KODACT_NUDGES`). A switch
//! writes the file; each chat's process restarts on its next message, so the
//! change applies without restarting the app.

use serde_json::{Map, Value};

/// One switch: its key, its name in Settings and what it does.
pub struct Item {
    pub key: &'static str,
    pub title: &'static str,
    pub hint: &'static str,
}

/// KodAct CLI's nudges (`NudgeConfig::NAMES`, in its order), each told as
/// what the model hears and when.
pub const NUDGES: [Item; 11] = [
    Item {
        key: "shellEdit",
        title: "Broken file after a shell edit",
        hint: "A shell command left a file it changed unparseable: KodAct says where the parse error is and suggests Edit.",
    },
    Item {
        key: "constraints",
        title: "What the request names",
        hint: "Before a turn ends: files the request names that don't exist, commands it names that never ran, a size limit a file is over.",
    },
    Item {
        key: "namedChecks",
        title: "Checks the request names",
        hint: "A check command the request names that never ran is pointed out (KodAct never runs it itself).",
    },
    Item {
        key: "passingSnapshots",
        title: "Last passing version",
        hint: "After a check passes the files are saved; if the last check then fails, KodAct says where the passing version is.",
    },
    Item {
        key: "limitVerdicts",
        title: "Limits met or missed",
        hint: "A command's key=value output is held against the limits the request states: each one met or not, and by how much.",
    },
    Item {
        key: "staleDeliverables",
        title: "Best result not saved",
        hint: "A measurement met every limit, but the file the request asks for wasn't written since: KodAct says so.",
    },
    Item {
        key: "adaptiveEffort",
        title: "Adaptive reasoning level",
        hint: "The reasoning level follows how the last steps went: down after thinking that led nowhere, back up after progress. Only with a chosen effort level, not Auto.",
    },
    Item {
        key: "thinkingTimeBox",
        title: "Thinking time-box",
        hint: "A reply that is still all thinking past a limit set by the run's own pace is stopped, its notes kept, and KodAct asked for one concrete step.",
    },
    Item {
        key: "todoTracking",
        title: "Task list reminders",
        hint: "The task list kept as the reference CLI does: a note when it is empty, and a reminder after 10 replies without updating it.",
    },
    Item {
        key: "clock",
        title: "Clock",
        hint: "In a run with a time limit, every tenth of it: the time used and left, and the files still missing.",
    },
    Item {
        key: "unpinnedInstalls",
        title: "Unpinned installs",
        hint: "Before a turn ends: a Python package installed with no version that nothing pins, and the version the checks ran against.",
    },
];

/// The reasoning carry-over.
pub const CARRY: Item = Item {
    key: "carryReasoning",
    title: "Carry reasoning",
    hint: "Sends the model's earlier reasoning back with each request, where the endpoint drops it (OpenAI-compatible chat completions), as DeepSeek's thinking mode asks for when tools are used.",
};

/// Each switch's state, as `settings.json` has it: `(key, on)` for every
/// nudge in [`NUDGES`] order, then [`CARRY`]. Missing keys are off, as
/// KodAct CLI treats them in a chat.
pub fn read(root: &Map<String, Value>) -> Vec<(&'static str, bool)> {
    let nudges = root.get("nudges");
    let nudge_on = |key: &str| match nudges {
        Some(Value::Bool(all)) => *all,
        Some(Value::Object(m)) => m.get(key).and_then(Value::as_bool).unwrap_or(false),
        Some(Value::String(list)) => list
            .split(',')
            .map(str::trim)
            .any(|n| n.eq_ignore_ascii_case(key) || n.eq_ignore_ascii_case("all")),
        _ => false,
    };
    NUDGES
        .iter()
        .map(|i| (i.key, nudge_on(i.key)))
        .chain(std::iter::once((
            CARRY.key,
            root.get(CARRY.key).and_then(Value::as_bool).unwrap_or(false),
        )))
        .collect()
}

/// Turn one switch on or off in `root`, keeping every other key. Only the
/// switch turned is written: a nudge the file never named keeps KodAct
/// CLI's own default (the terminal UI turns some on). A `nudges` written
/// as `true`, `false` or a list becomes the object form, the others as it
/// had them.
pub fn set(root: &mut Map<String, Value>, key: &str, on: bool) {
    if key == CARRY.key {
        root.insert(key.into(), Value::Bool(on));
        return;
    }
    let before = read(root);
    let mut nudges = match root.remove("nudges") {
        Some(Value::Object(m)) => m,
        None | Some(Value::Null) => Map::new(),
        _ => before
            .iter()
            .filter(|(k, _)| *k != CARRY.key)
            .map(|(k, v)| ((*k).to_string(), Value::Bool(*v)))
            .collect(),
    };
    nudges.insert(key.into(), Value::Bool(on));
    root.insert("nudges".into(), Value::Object(nudges));
}

/// The file the switches read and write: KodAct's own `settings.json`.
pub fn path() -> Option<std::path::PathBuf> {
    crate::forge_files::cli_home().map(|h| h.join("settings.json"))
}

/// Each switch's state from the file (all off when there is none).
pub fn load() -> Vec<(&'static str, bool)> {
    let root = path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    read(&root)
}

/// Write one switch to the file.
pub fn save(key: &str, on: bool) -> Result<(), String> {
    let p = path().ok_or("KodAct's config folder wasn't found")?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    crate::forge_files::edit_json(&p, |root| set(root, key, on))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn every_nudge_kodact_cli_knows_has_a_switch() {
        let keys: Vec<&str> = NUDGES.iter().map(|i| i.key).collect();
        assert_eq!(keys, crate::backend::ALL_NUDGES);
    }

    #[test]
    fn reads_each_form_kodact_cli_accepts() {
        let on = |v: Value, key: &str| read(&obj(v)).iter().any(|(k, o)| *k == key && *o);
        assert!(on(json!({"nudges": {"clock": true}}), "clock"));
        assert!(!on(json!({"nudges": {"clock": true}}), "constraints"));
        assert!(on(json!({"nudges": true}), "shellEdit"));
        assert!(on(json!({"nudges": "constraints, clock"}), "clock"));
        assert!(on(json!({"carryReasoning": true}), "carryReasoning"));
        assert!(!on(json!({}), "carryReasoning"));
        assert_eq!(read(&Map::new()).len(), 12);
    }

    #[test]
    fn a_switch_changes_only_its_key() {
        let mut root = obj(json!({"model": "x", "nudges": "constraints,clock"}));
        set(&mut root, "clock", false);
        set(&mut root, "shellEdit", true);
        set(&mut root, "carryReasoning", true);
        assert_eq!(root["model"], "x");
        assert_eq!(root["nudges"]["constraints"], true);
        assert_eq!(root["nudges"]["clock"], false);
        assert_eq!(root["nudges"]["shellEdit"], true);
        assert_eq!(root["carryReasoning"], true);
        // No nudges named yet: only the one switched is written.
        let mut root = obj(json!({}));
        set(&mut root, "clock", true);
        assert_eq!(root["nudges"], json!({"clock": true}));
    }

    #[test]
    fn saves_to_the_file_kodact_cli_reads() {
        let dir = std::env::temp_dir().join(format!("kodact-behaviour-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("settings.json");
        std::fs::write(&p, r#"{"theme": "dark"}"#).unwrap();
        crate::forge_files::edit_json(&p, |r| set(r, "thinkingTimeBox", true)).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["nudges"]["thinkingTimeBox"], true);
        // KodAct CLI reads the file itself; the desktop sets no variable over it.
        assert_eq!(crate::backend::nudges_for_settings(None, &[v], false), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
