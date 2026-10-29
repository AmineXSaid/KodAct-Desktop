//! Forge's modes: Manual, Edit automatically, Auto, Plan, Don't ask and
//! Bypass permissions, one per ForgeCLI permission mode.
//!
//! Shift+Tab goes round Manual, Edit automatically and Plan (and Bypass once
//! allowed), as ForgeCLI's own prompt does; Auto and Don't ask are picked
//! from the menu. Bypass is offered only once the person has allowed it,
//! behind a warning. Teaching is not a mode: it
//! is the bundled `expert` skill (`/forge:expert`), which works in any mode.

/// A permission mode, as the CLI names it (`--permission-mode`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Permission {
    Default,
    AcceptEdits,
    Plan,
    DontAsk,
    BypassPermissions,
    Auto,
}

impl Permission {
    pub fn cli_name(self) -> &'static str {
        match self {
            Permission::Default => "default",
            Permission::AcceptEdits => "acceptEdits",
            Permission::Plan => "plan",
            Permission::DontAsk => "dontAsk",
            Permission::Auto => "auto",
            Permission::BypassPermissions => "bypassPermissions",
        }
    }
}

/// A row of the Modes menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModeId {
    Manual,
    AcceptEdits,
    Auto,
    Plan,
    DontAsk,
    Bypass,
}

impl ModeId {
    /// Every row, in menu order.
    pub const ALL: [ModeId; 6] = [
        ModeId::Manual,
        ModeId::AcceptEdits,
        ModeId::Auto,
        ModeId::Plan,
        ModeId::DontAsk,
        ModeId::Bypass,
    ];

    /// The Shift+Tab round; Bypass joins it once allowed.
    const ROUND: [ModeId; 4] = [
        ModeId::Manual,
        ModeId::AcceptEdits,
        ModeId::Plan,
        ModeId::Bypass,
    ];

    /// A mode as ForgeCLI names it (`permissionMode` in its events).
    pub fn from_cli(name: &str) -> Option<ModeId> {
        ModeId::ALL
            .into_iter()
            .find(|m| m.permission().cli_name() == name)
            .or((name == "manual").then_some(ModeId::Manual))
    }

    /// How a project keeps it (`projects.json`).
    pub fn key(self) -> &'static str {
        match self {
            ModeId::Manual => "manual",
            ModeId::AcceptEdits => "accept_edits",
            ModeId::Auto => "auto",
            ModeId::Plan => "plan",
            ModeId::DontAsk => "dont_ask",
            ModeId::Bypass => "bypass",
        }
    }

    /// A kept mode. The Expert mode some projects and chats saved is Manual
    /// now (teaching moved to the `expert` skill).
    pub fn from_key(key: &str) -> Option<ModeId> {
        if key == "expert" {
            return Some(ModeId::Manual);
        }
        ModeId::ALL.into_iter().find(|m| m.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            ModeId::Manual => "Manual",
            ModeId::AcceptEdits => "Edit automatically",
            ModeId::Auto => "Auto",
            ModeId::Plan => "Plan",
            ModeId::DontAsk => "Don't ask",
            ModeId::Bypass => "Bypass permissions",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ModeId::Manual => "Forge will ask for approval before making each edit",
            ModeId::AcceptEdits => "Forge will edit your selected text or the whole file",
            ModeId::Auto => {
                "Forge will edit in the project and run routine commands; risky ones still ask"
            }
            ModeId::Plan => "Forge will explore the code and present a plan before editing",
            ModeId::DontAsk => {
                "Forge will never ask: it does what your rules allow and skips the rest"
            }
            ModeId::Bypass => {
                "Forge will not ask for approval before running potentially dangerous commands"
            }
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ModeId::Manual => "icons/hand.svg",
            ModeId::AcceptEdits => "icons/code-xml.svg",
            ModeId::Auto => "icons/wand-sparkles.svg",
            ModeId::Plan => "icons/scroll-text.svg",
            ModeId::DontAsk => "icons/shield-x.svg",
            ModeId::Bypass => "icons/route.svg",
        }
    }

    /// What the CLI runs in.
    pub fn permission(self) -> Permission {
        match self {
            ModeId::Manual => Permission::Default,
            ModeId::AcceptEdits => Permission::AcceptEdits,
            ModeId::Auto => Permission::Auto,
            ModeId::Plan => Permission::Plan,
            ModeId::DontAsk => Permission::DontAsk,
            ModeId::Bypass => Permission::BypassPermissions,
        }
    }

    /// Whether a chat keeps this mode when it is reopened. As in the
    /// extension, every mode but Plan is kept (Bypass only while it is
    /// allowed); Plan lasts for the session.
    pub fn kept(self, bypass_allowed: bool) -> bool {
        match self {
            ModeId::Manual | ModeId::AcceptEdits | ModeId::Auto | ModeId::DontAsk => true,
            ModeId::Bypass => bypass_allowed,
            ModeId::Plan => false,
        }
    }
}

/// Shift+Tab: the next mode, with Bypass in the round only when allowed. A
/// mode outside the round (Auto, Don't ask) goes back to Manual.
pub fn next(current: ModeId, bypass_allowed: bool) -> ModeId {
    let order: &[ModeId] = if bypass_allowed {
        &ModeId::ROUND
    } else {
        &ModeId::ROUND[..3]
    };
    if !order.contains(&current) {
        return ModeId::Manual;
    }
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

/// `--settings` for a chat's process: the settings profile's file, if any.
pub fn flag_settings(profile: Option<&std::path::Path>) -> Option<String> {
    profile.map(|p| p.to_string_lossy().into_owned())
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
        assert_eq!(next(ModeId::Manual, false), ModeId::AcceptEdits);
        assert_eq!(next(ModeId::AcceptEdits, false), ModeId::Plan);
        assert_eq!(next(ModeId::Plan, false), ModeId::Manual);
        assert_eq!(next(ModeId::Plan, true), ModeId::Bypass);
        assert_eq!(next(ModeId::Bypass, true), ModeId::Manual);
        // A mode outside the round starts it again.
        assert_eq!(next(ModeId::Bypass, false), ModeId::Manual);
        assert_eq!(next(ModeId::Auto, true), ModeId::Manual);
        assert_eq!(next(ModeId::DontAsk, false), ModeId::Manual);
    }

    #[test]
    fn every_forgecli_permission_mode_has_a_row() {
        for name in ["default", "acceptEdits", "plan", "dontAsk", "bypassPermissions", "auto"] {
            let m = ModeId::from_cli(name).unwrap();
            assert_eq!(m.permission().cli_name(), name);
        }
        assert_eq!(ModeId::from_cli("manual"), Some(ModeId::Manual));
        assert_eq!(ModeId::from_cli("yolo"), None);
        assert_eq!(cli_args(ModeId::Auto, false), ["--permission-mode", "auto"]);
        assert_eq!(cli_args(ModeId::DontAsk, false), ["--permission-mode", "dontAsk"]);
    }

    #[test]
    fn a_saved_expert_mode_is_manual_now() {
        assert_eq!(ModeId::from_key("expert"), Some(ModeId::Manual));
        for m in ModeId::ALL {
            assert_eq!(ModeId::from_key(m.key()), Some(m));
        }
        assert_eq!(ModeId::from_key("nonsense"), None);
    }

    #[test]
    fn what_the_cli_is_given() {
        assert_eq!(
            cli_args(ModeId::Manual, false),
            ["--permission-mode", "default"]
        );
        assert_eq!(cli_args(ModeId::Plan, false), ["--permission-mode", "plan"]);
        assert_eq!(
            cli_args(ModeId::AcceptEdits, true),
            [
                "--permission-mode",
                "acceptEdits",
                "--allow-dangerously-skip-permissions"
            ]
        );
        assert_eq!(
            cli_args(ModeId::Bypass, true),
            [
                "--permission-mode",
                "bypassPermissions",
                "--allow-dangerously-skip-permissions"
            ]
        );
        assert_eq!(initial(ModeId::Bypass, false), ModeId::Manual);
        assert_eq!(initial(ModeId::Bypass, true), ModeId::Bypass);
    }

    #[test]
    fn only_some_modes_are_kept() {
        assert!(ModeId::Manual.kept(false) && ModeId::AcceptEdits.kept(false));
        assert!(!ModeId::Plan.kept(true));
        assert!(ModeId::Bypass.kept(true) && !ModeId::Bypass.kept(false));
    }

    #[test]
    fn the_profile_goes_as_flag_settings() {
        assert_eq!(flag_settings(None), None);
        let p = std::path::Path::new("/x/work.json");
        assert_eq!(flag_settings(Some(p)).as_deref(), Some("/x/work.json"));
    }
}
