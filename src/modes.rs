//! KodAct's modes, as KodAct for VS Code has them: Manual, Edit
//! automatically, Plan, and Bypass permissions.
//!
//! All four are Claude Code permission modes. Bypass is offered only once
//! the person has allowed it, behind a warning. Teaching is not a mode: it
//! is the bundled `expert` skill (`/kodact:expert`), which works in any mode.

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
    Manual,
    AcceptEdits,
    Plan,
    Bypass,
}

impl ModeId {
    /// Every row, in menu order.
    pub const ALL: [ModeId; 4] = [
        ModeId::Manual,
        ModeId::AcceptEdits,
        ModeId::Plan,
        ModeId::Bypass,
    ];

    /// How a project keeps it (`projects.json`).
    pub fn key(self) -> &'static str {
        match self {
            ModeId::Manual => "manual",
            ModeId::AcceptEdits => "accept_edits",
            ModeId::Plan => "plan",
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
            ModeId::Plan => "Plan",
            ModeId::Bypass => "Bypass permissions",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ModeId::Manual => "KodAct will ask for approval before making each edit",
            ModeId::AcceptEdits => "KodAct will edit your selected text or the whole file",
            ModeId::Plan => "KodAct will explore the code and present a plan before editing",
            ModeId::Bypass => {
                "KodAct will not ask for approval before running potentially dangerous commands"
            }
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ModeId::Manual => "icons/hand.svg",
            ModeId::AcceptEdits => "icons/code-xml.svg",
            ModeId::Plan => "icons/scroll-text.svg",
            ModeId::Bypass => "icons/route.svg",
        }
    }

    /// What the CLI runs in.
    pub fn permission(self) -> Permission {
        match self {
            ModeId::Manual => Permission::Default,
            ModeId::AcceptEdits => Permission::AcceptEdits,
            ModeId::Plan => Permission::Plan,
            ModeId::Bypass => Permission::BypassPermissions,
        }
    }

    /// Whether a chat keeps this mode when it is reopened. As in the
    /// extension, only Manual, Edit automatically and Bypass are kept (Bypass
    /// only while it is allowed); Plan lasts for the session.
    pub fn kept(self, bypass_allowed: bool) -> bool {
        match self {
            ModeId::Manual | ModeId::AcceptEdits => true,
            ModeId::Bypass => bypass_allowed,
            ModeId::Plan => false,
        }
    }
}

/// Shift+Tab: the next mode, with Bypass in the round only when allowed.
pub fn next(current: ModeId, bypass_allowed: bool) -> ModeId {
    let order: &[ModeId] = if bypass_allowed {
        &ModeId::ALL
    } else {
        &ModeId::ALL[..3]
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

/// `--settings` for a chat's process: the settings profile's file, if any.
pub fn flag_settings(profile: Option<&std::path::Path>) -> Option<String> {
    profile.map(|p| p.to_string_lossy().into_owned())
}

/// The flags a chat's `kodact` process starts with for `mode`. While Bypass
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
        // A mode that left the round (Bypass, now disallowed) starts it again.
        assert_eq!(next(ModeId::Bypass, false), ModeId::AcceptEdits);
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
