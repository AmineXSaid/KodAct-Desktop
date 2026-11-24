//! KodAct's modes: Manual, Edit automatically, Auto, Plan, and Bypass
//! permissions, in the order the KodAct CLI's Shift+Tab cycles them. Don't
//! ask is shown when KodAct CLI switches to it, but isn't in the cycle.
//!
//! All are KodAct CLI permission modes (`--permission-mode`). Bypass is offered only once
//! the person has allowed it, behind a warning. Teaching is not a mode: it
//! is the bundled `expert` skill (`/kodact:expert`), which works in any mode.

/// A permission mode, as the CLI names it (`--permission-mode`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Permission {
    Default,
    AcceptEdits,
    Plan,
    BypassPermissions,
    /// Edits and routine commands run; removals and risky commands ask.
    Auto,
    /// Never asks: what would ask is denied.
    DontAsk,
}

impl Permission {
    pub fn cli_name(self) -> &'static str {
        match self {
            Permission::Default => "default",
            Permission::AcceptEdits => "acceptEdits",
            Permission::Plan => "plan",
            Permission::BypassPermissions => "bypassPermissions",
            Permission::Auto => "auto",
            Permission::DontAsk => "dontAsk",
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
    Bypass,
    DontAsk,
}

impl ModeId {
    /// Every row the menu offers, in menu (and Shift+Tab) order. Don't ask
    /// isn't offered: it is listed only while a chat is in it.
    pub const ALL: [ModeId; 5] = [
        ModeId::Manual,
        ModeId::AcceptEdits,
        ModeId::Auto,
        ModeId::Plan,
        ModeId::Bypass,
    ];

    /// The mode KodAct CLI names `name` (`system/status`'s `permissionMode`).
    pub fn from_cli(name: &str) -> Option<ModeId> {
        Some(match name {
            "default" | "manual" => ModeId::Manual,
            "acceptEdits" => ModeId::AcceptEdits,
            "auto" => ModeId::Auto,
            "plan" => ModeId::Plan,
            "bypassPermissions" => ModeId::Bypass,
            "dontAsk" => ModeId::DontAsk,
            _ => return None,
        })
    }

    /// How a project keeps it (`projects.json`).
    pub fn key(self) -> &'static str {
        match self {
            ModeId::Manual => "manual",
            ModeId::AcceptEdits => "accept_edits",
            ModeId::Auto => "auto",
            ModeId::Plan => "plan",
            ModeId::Bypass => "bypass",
            ModeId::DontAsk => "dont_ask",
        }
    }

    /// A kept mode. The Expert mode some projects and chats saved is Manual
    /// now (teaching moved to the `expert` skill).
    pub fn from_key(key: &str) -> Option<ModeId> {
        if key == "expert" {
            return Some(ModeId::Manual);
        }
        ModeId::ALL
            .into_iter()
            .chain([ModeId::DontAsk])
            .find(|m| m.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            ModeId::Manual => "Manual",
            ModeId::AcceptEdits => "Edit automatically",
            ModeId::Auto => "Auto",
            ModeId::Plan => "Plan",
            ModeId::Bypass => "Bypass permissions",
            ModeId::DontAsk => "Don't ask",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ModeId::Manual => "KodAct will ask for approval before making each edit",
            ModeId::AcceptEdits => "KodAct will edit your selected text or the whole file",
            ModeId::Auto => {
                "KodAct will edit files and run routine commands, and ask before removals and risky commands"
            }
            ModeId::DontAsk => {
                "KodAct will never ask: anything that would need your approval is refused"
            }
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
            ModeId::Auto => "icons/zap.svg",
            ModeId::Plan => "icons/scroll-text.svg",
            ModeId::Bypass => "icons/route.svg",
            ModeId::DontAsk => "icons/shield-x.svg",
        }
    }

    /// What the CLI runs in.
    pub fn permission(self) -> Permission {
        match self {
            ModeId::Manual => Permission::Default,
            ModeId::AcceptEdits => Permission::AcceptEdits,
            ModeId::Auto => Permission::Auto,
            ModeId::Plan => Permission::Plan,
            ModeId::Bypass => Permission::BypassPermissions,
            ModeId::DontAsk => Permission::DontAsk,
        }
    }

    /// Whether a chat keeps this mode when it is reopened: Manual, Edit
    /// automatically, Auto and Bypass (Bypass only while it is allowed).
    /// Plan lasts for the session, and so does Don't ask, which only KodAct
    /// CLI switches to.
    pub fn kept(self, bypass_allowed: bool) -> bool {
        match self {
            ModeId::Manual | ModeId::AcceptEdits | ModeId::Auto => true,
            ModeId::Bypass => bypass_allowed,
            ModeId::Plan | ModeId::DontAsk => false,
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
        assert_eq!(next(ModeId::AcceptEdits, false), ModeId::Auto);
        assert_eq!(next(ModeId::Auto, false), ModeId::Plan);
        assert_eq!(next(ModeId::Plan, false), ModeId::Manual);
        // Don't ask, which KodAct CLI chose, isn't in the round either.
        assert_eq!(next(ModeId::DontAsk, false), ModeId::AcceptEdits);
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
        assert_eq!(ModeId::from_key("dont_ask"), Some(ModeId::DontAsk));
        assert_eq!(ModeId::from_key("nonsense"), None);
    }

    #[test]
    fn every_mode_the_cli_reports_is_known() {
        for m in ModeId::ALL.into_iter().chain([ModeId::DontAsk]) {
            assert_eq!(ModeId::from_cli(m.permission().cli_name()), Some(m));
        }
        assert_eq!(ModeId::from_cli("auto"), Some(ModeId::Auto));
        assert_eq!(ModeId::from_cli("dontAsk"), Some(ModeId::DontAsk));
        assert_eq!(cli_args(ModeId::Auto, false), ["--permission-mode", "auto"]);
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
        assert!(ModeId::Auto.kept(false));
        assert!(!ModeId::Plan.kept(true) && !ModeId::DontAsk.kept(true));
        assert!(ModeId::Bypass.kept(true) && !ModeId::Bypass.kept(false));
    }

    #[test]
    fn the_profile_goes_as_flag_settings() {
        assert_eq!(flag_settings(None), None);
        let p = std::path::Path::new("/x/work.json");
        assert_eq!(flag_settings(Some(p)).as_deref(), Some("/x/work.json"));
    }
}
