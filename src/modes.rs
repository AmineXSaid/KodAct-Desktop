//! Forge's modes, as Forge for VS Code has them: Expert, Manual, Edit
//! automatically, Plan, and Bypass permissions.
//!
//! Four are Claude Code permission modes. Expert is Forge's own: Manual
//! underneath, plus the bundled Expert output style (Forge teaches step by
//! step and asks before each edit). Bypass is offered only once the person
//! has allowed it, behind a warning.

/// A permission mode, as the CLI names it (`--permission-mode`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Permission {
    Default,
    AcceptEdits,
    Plan,
    BypassPermissions,
}

impl Permission {
    pub fn cli_name(self) -> &'static str {
        match self {
            Permission::Default => "default",
            Permission::AcceptEdits => "acceptEdits",
            Permission::Plan => "plan",
            Permission::BypassPermissions => "bypassPermissions",
        }
    }
}

/// A row of the Modes menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModeId {
    Expert,
    Manual,
    AcceptEdits,
    Plan,
    Bypass,
}

/// The output style Expert turns on (`output-styles/expert.md` in the
/// bundled plugin; ForgeCLI finds plugin styles by their plain name).
pub const EXPERT_OUTPUT_STYLE: &str = "Expert";
/// Its text, for when the CLI is launched with Forge's plugin.
#[cfg_attr(not(test), allow(dead_code))]
pub const EXPERT_STYLE_MD: &str = include_str!("../assets/forge-plugin/output-styles/expert.md");

impl ModeId {
    /// Every row, in menu order.
    pub const ALL: [ModeId; 5] = [
        ModeId::Expert,
        ModeId::Manual,
        ModeId::AcceptEdits,
        ModeId::Plan,
        ModeId::Bypass,
    ];

    /// How a project keeps it (`projects.json`).
    pub fn key(self) -> &'static str {
        match self {
            ModeId::Expert => "expert",
            ModeId::Manual => "manual",
            ModeId::AcceptEdits => "accept_edits",
            ModeId::Plan => "plan",
            ModeId::Bypass => "bypass",
        }
    }

    pub fn from_key(key: &str) -> Option<ModeId> {
        ModeId::ALL.into_iter().find(|m| m.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            ModeId::Expert => "Expert",
            ModeId::Manual => "Manual",
            ModeId::AcceptEdits => "Edit automatically",
            ModeId::Plan => "Plan",
            ModeId::Bypass => "Bypass permissions",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ModeId::Expert => {
                "Forge teaches step by step, from the basics up, and asks before each edit"
            }
            ModeId::Manual => "Forge will ask for approval before making each edit",
            ModeId::AcceptEdits => "Forge will edit your selected text or the whole file",
            ModeId::Plan => "Forge will explore the code and present a plan before editing",
            ModeId::Bypass => {
                "Forge will not ask for approval before running potentially dangerous commands"
            }
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ModeId::Expert => "icons/graduation-cap.svg",
            ModeId::Manual => "icons/hand.svg",
            ModeId::AcceptEdits => "icons/code-xml.svg",
            ModeId::Plan => "icons/scroll-text.svg",
            ModeId::Bypass => "icons/route.svg",
        }
    }

    /// What the CLI runs in: Expert is Manual underneath.
    pub fn permission(self) -> Permission {
        match self {
            ModeId::Expert | ModeId::Manual => Permission::Default,
            ModeId::AcceptEdits => Permission::AcceptEdits,
            ModeId::Plan => Permission::Plan,
            ModeId::Bypass => Permission::BypassPermissions,
        }
    }

    /// The session's output style: Expert's, else none (cleared).
    pub fn output_style(self) -> Option<&'static str> {
        (self == ModeId::Expert).then_some(EXPERT_OUTPUT_STYLE)
    }

    /// Whether a chat keeps this mode when it is reopened. As in the
    /// extension, only Manual, Edit automatically and Bypass are kept (Bypass
    /// only while it is allowed); Plan and Expert last for the session.
    pub fn kept(self, bypass_allowed: bool) -> bool {
        match self {
            ModeId::Manual | ModeId::AcceptEdits => true,
            ModeId::Bypass => bypass_allowed,
            ModeId::Plan | ModeId::Expert => false,
        }
    }
}

/// Shift+Tab: the next mode, with Bypass in the round only when allowed.
pub fn next(current: ModeId, bypass_allowed: bool) -> ModeId {
    let order: &[ModeId] = if bypass_allowed {
        &ModeId::ALL
    } else {
        &ModeId::ALL[..4]
    };
    let at = order.iter().position(|m| *m == current).unwrap_or(0);
    order[(at + 1) % order.len()]
}

/// A new chat's mode from the setting: Bypass falls back to Manual when it
/// isn't allowed.
pub fn initial(setting: ModeId, bypass_allowed: bool) -> ModeId {
    match setting {
        ModeId::Bypass if !bypass_allowed => ModeId::Manual,
        m => m,
    }
}

/// `--settings` for a chat's process: the settings profile's file as it is,
/// or, in Expert, the profile's settings (if any) with Expert's output style
/// over them, as JSON. None: no flag settings.
pub fn flag_settings(mode: ModeId, profile: Option<&std::path::Path>) -> Option<String> {
    let Some(style) = mode.output_style() else {
        return profile.map(|p| p.to_string_lossy().into_owned());
    };
    let mut settings = profile
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    settings["outputStyle"] = serde_json::Value::from(style);
    Some(settings.to_string())
}

/// The flags a chat's `forge` process starts with for `mode`. While Bypass
/// is allowed the switch is always passed, so the mode can change mid-session.
pub fn cli_args(mode: ModeId, bypass_allowed: bool) -> Vec<String> {
    let mut args = vec![
        "--permission-mode".to_string(),
        mode.permission().cli_name().to_string(),
    ];
    if bypass_allowed {
        args.push("--allow-dangerously-skip-permissions".into());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_tab_cycles_and_skips_bypass_until_allowed() {
        assert_eq!(next(ModeId::Expert, false), ModeId::Manual);
        assert_eq!(next(ModeId::Plan, false), ModeId::Expert);
        assert_eq!(next(ModeId::Plan, true), ModeId::Bypass);
        assert_eq!(next(ModeId::Bypass, true), ModeId::Expert);
        // A mode that left the round (Bypass, now disallowed) starts it again.
        assert_eq!(next(ModeId::Bypass, false), ModeId::Manual);
    }

    #[test]
    fn expert_is_manual_with_its_style() {
        assert_eq!(ModeId::Expert.permission(), Permission::Default);
        assert_eq!(ModeId::Expert.output_style(), Some("Expert"));
        assert_eq!(ModeId::Manual.output_style(), None);
        assert!(EXPERT_STYLE_MD.contains("name: Expert"));
    }

    #[test]
    fn what_the_cli_is_given() {
        assert_eq!(cli_args(ModeId::Plan, false), ["--permission-mode", "plan"]);
        assert_eq!(
            cli_args(ModeId::AcceptEdits, true),
            [
                "--permission-mode",
                "acceptEdits",
                "--allow-dangerously-skip-permissions"
            ]
        );
        assert_eq!(initial(ModeId::Bypass, false), ModeId::Manual);
    }

    #[test]
    fn only_some_modes_are_kept() {
        assert!(ModeId::Manual.kept(false) && ModeId::AcceptEdits.kept(false));
        assert!(!ModeId::Plan.kept(true) && !ModeId::Expert.kept(true));
        assert!(ModeId::Bypass.kept(true) && !ModeId::Bypass.kept(false));
    }

    #[test]
    fn expert_launches_with_its_output_style_over_the_profile() {
        assert_eq!(flag_settings(ModeId::Manual, None), None);
        assert_eq!(
            flag_settings(ModeId::Expert, None).as_deref(),
            Some(r#"{"outputStyle":"Expert"}"#)
        );
        let dir = std::env::temp_dir().join(format!("forge-modes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let profile = dir.join("work.json");
        std::fs::write(&profile, r#"{"model":"m","outputStyle":"Terse"}"#).unwrap();
        assert_eq!(
            flag_settings(ModeId::Plan, Some(&profile)),
            Some(profile.to_string_lossy().into_owned()),
            "other modes pass the profile's file"
        );
        let v: serde_json::Value =
            serde_json::from_str(&flag_settings(ModeId::Expert, Some(&profile)).unwrap()).unwrap();
        assert_eq!(v["outputStyle"], "Expert");
        assert_eq!(v["model"], "m", "the profile's settings stay");
        std::fs::remove_dir_all(dir).ok();
    }
}
