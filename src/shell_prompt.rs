//! Colour for the terminal dock's shells.
//!
//! A shell started bare prints in one colour: bash and sh with a stock
//! prompt, PowerShell's `PS C:\>`, cmd's `C:\>`. Each local shell the dock
//! starts gets a prompt in the dock's ANSI colours (so it follows the theme),
//! and bash, zsh and sh colour `ls` and `grep`, unless the person's own
//! startup files already set a coloured prompt: those load first and win.
//!
//! - bash: `--rcfile`, a file that sources `~/.bashrc` first.
//! - zsh: `ZDOTDIR`, a folder whose `.zshenv` and `.zshrc` source the real ones.
//! - sh, dash: `ENV`, read by an interactive POSIX shell, unless already set.
//! - PowerShell: `-NoExit -EncodedCommand`, inline (a script file would need
//!   an execution policy that allows scripts; Windows' default doesn't).
//! - cmd: `PROMPT`, unless already set.
//!
//! Other programs (fish, which colours itself; ssh, wsl and docker for
//! remote shells) are left as they are.

use std::path::{Path, PathBuf};

/// A shell's command line once dressed: what to run and what to add to its
/// environment.
#[derive(Debug, Clone, PartialEq)]
pub struct Dressed {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The kinds of shell this module knows how to colour.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Bash,
    Zsh,
    Posix,
    PowerShell,
    Cmd,
}

fn kind(program: &str) -> Option<Kind> {
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    Some(match name {
        "bash" => Kind::Bash,
        "zsh" => Kind::Zsh,
        "sh" | "dash" | "ash" | "ksh" | "mksh" => Kind::Posix,
        "powershell" | "pwsh" => Kind::PowerShell,
        "cmd" => Kind::Cmd,
        _ => return None,
    })
}

/// bash: the person's `~/.bashrc`, then a prompt in colour where theirs has
/// none, and `ls` and `grep` in colour where they aren't aliased.
const BASH_RC: &str = r#"# KodAct's terminal: your startup file first, then colour where it set none.
[ -f ~/.bashrc ] && . ~/.bashrc
# A colour code (`\e[`) means the prompt is coloured already; a title alone
# (`\e]0;…`, as Ubuntu's stock prompt sets) does not.
case "$PS1" in
  *\\e\[*|*\\033\[*|*\\x1b\[*|*$'\e['*) ;;
  *) PS1='\[\e]0;\u@\h: \w\a\]\[\e[1;32m\]\u@\h\[\e[0m\]:\[\e[1;34m\]\w\[\e[0m\]\$ ' ;;
esac
if [ -z "$LS_COLORS" ] && command -v dircolors >/dev/null 2>&1; then eval "$(dircolors -b)"; fi
export CLICOLOR=1
alias ls >/dev/null 2>&1 || { ls --color=auto -d . >/dev/null 2>&1 && alias ls='ls --color=auto'; }
alias grep >/dev/null 2>&1 || { echo | grep --color=auto x >/dev/null 2>&1; [ $? -le 1 ] && alias grep='grep --color=auto'; }
"#;

/// zsh reads `.zshenv` then `.zshrc` from `ZDOTDIR`: these pass on to the
/// person's own (from `KODACT_USER_ZDOTDIR`, else home), then colour.
const ZSH_ENV: &str = r#"# KodAct's terminal: your own .zshenv.
__forge_z="${KODACT_USER_ZDOTDIR:-$HOME}"
[ -f "$__forge_z/.zshenv" ] && ZDOTDIR="$__forge_z" . "$__forge_z/.zshenv"
"#;
const ZSH_RC: &str = r#"# KodAct's terminal: your own .zshrc, then colour where it set none.
__forge_z="${KODACT_USER_ZDOTDIR:-$HOME}"
ZDOTDIR="$__forge_z"
[ -f "$__forge_z/.zshrc" ] && . "$__forge_z/.zshrc"
unset __forge_z
case "$PROMPT" in
  *%F\{*|*%K\{*|*$'\e['*) ;;
  *) PROMPT='%B%F{green}%n@%m%f%b:%B%F{blue}%~%f%b%# ' ;;
esac
export CLICOLOR=1
if [ -z "$LS_COLORS" ] && command -v dircolors >/dev/null 2>&1; then eval "$(dircolors -b)"; fi
alias ls >/dev/null 2>&1 || { ls --color=auto -d . >/dev/null 2>&1 && alias ls='ls --color=auto'; }
"#;

/// sh: `$ENV` is read by an interactive POSIX shell; its PS1 has no escapes
/// of its own, so the escape character is put in by printf.
const SH_ENV: &str = r#"# KodAct's terminal: a prompt in colour.
__forge_e=$(printf '\033')
__forge_s='$'; [ "$(id -u 2>/dev/null)" = 0 ] && __forge_s='#'
PS1="${__forge_e}[1;32m$(id -un 2>/dev/null)@$(hostname 2>/dev/null || uname -n)${__forge_e}[0m:${__forge_e}[1;34m"'$PWD'"${__forge_e}[0m${__forge_s} "
unset __forge_e __forge_s
ls --color=auto -d . >/dev/null 2>&1 && alias ls='ls --color=auto'
"#;

/// PowerShell: replaces only the stock prompt (`PS <path>>`); a profile's
/// own prompt stays.
const PS_PROMPT: &str = r#"if ((Get-Command prompt -CommandType Function -ErrorAction SilentlyContinue).ScriptBlock.ToString() -match 'PS \$\(\$executionContext') {
  function global:prompt {
    $e = [char]27
    "$e[1;32mPS$e[0m $e[1;34m$($executionContext.SessionState.Path.CurrentLocation)$e[0m$('>' * ($nestedPromptLevel + 1)) "
  }
}"#;

/// cmd: green path, then `>`.
const CMD_PROMPT: &str = "$E[1;32m$P$E[0m$G";

/// Where the startup files go: a folder of the app's in the temp folder.
fn files_dir() -> PathBuf {
    std::env::temp_dir().join("forge-terminal")
}

/// Write `text` to `dir/name` unless it is already there as is.
fn write_once(dir: &Path, name: &str, text: &str) -> Option<PathBuf> {
    let path = dir.join(name);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text) {
        std::fs::create_dir_all(dir).ok()?;
        std::fs::write(&path, text).ok()?;
    }
    Some(path)
}

/// PowerShell's `-EncodedCommand`: base64 of the UTF-16LE text.
fn encoded_command(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `program` and `args` with colour added, as the module's notes say; as
/// they were when the shell is not one it knows, already has arguments of
/// its own, or a startup file can't be written.
pub fn dress(program: &str, args: &[String]) -> Dressed {
    dress_in(program, args, &files_dir(), |k| std::env::var(k).ok())
}

fn dress_in(
    program: &str,
    args: &[String],
    dir: &Path,
    var: impl Fn(&str) -> Option<String>,
) -> Dressed {
    let plain = Dressed {
        program: program.to_string(),
        args: args.to_vec(),
        env: vec![],
    };
    let Some(kind) = kind(program) else {
        return plain;
    };
    // Only a shell started as the dock starts it: bare, or PowerShell with
    // just -NoLogo. Anything else is the person's own command line.
    let bare = args.is_empty() || (kind == Kind::PowerShell && args == ["-NoLogo"]);
    if !bare {
        return plain;
    }
    let mut out = plain.clone();
    match kind {
        Kind::Bash => {
            let Some(rc) = write_once(dir, "bashrc", BASH_RC) else {
                return plain;
            };
            out.args = vec!["--rcfile".into(), rc.to_string_lossy().into_owned()];
        }
        Kind::Zsh => {
            let zdot = dir.join("zsh");
            if write_once(&zdot, ".zshenv", ZSH_ENV).is_none()
                || write_once(&zdot, ".zshrc", ZSH_RC).is_none()
            {
                return plain;
            }
            if let Some(own) = var("ZDOTDIR") {
                out.env.push(("KODACT_USER_ZDOTDIR".into(), own));
            }
            out.env
                .push(("ZDOTDIR".into(), zdot.to_string_lossy().into_owned()));
        }
        Kind::Posix => {
            if var("ENV").is_some() {
                return plain;
            }
            let Some(env) = write_once(dir, "shrc", SH_ENV) else {
                return plain;
            };
            out.env
                .push(("ENV".into(), env.to_string_lossy().into_owned()));
        }
        Kind::PowerShell => {
            out.args.extend([
                "-NoExit".into(),
                "-EncodedCommand".into(),
                encoded_command(PS_PROMPT),
            ]);
        }
        Kind::Cmd => {
            if var("PROMPT").is_some() {
                return plain;
            }
            out.env.push(("PROMPT".into(), CMD_PROMPT.into()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<String> {
        None
    }

    /// A folder of the test's own: tests run in parallel and each removes its own.
    fn dir(test: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("forge-shell-prompt-{}-{test}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn base64_is_powershells() {
        // [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes("dir"))
        assert_eq!(encoded_command("dir"), "ZABpAHIA");
        assert_eq!(encoded_command("ab"), "YQBiAA==");
    }

    #[test]
    fn each_shell_is_dressed_its_own_way() {
        let d = dir("dressed");
        let bash = dress_in("/bin/bash", &[], &d, none);
        assert_eq!(bash.args[0], "--rcfile");
        assert!(std::fs::read_to_string(&bash.args[1])
            .unwrap()
            .contains(". ~/.bashrc"));

        let zsh = dress_in("zsh", &[], &d, |k| (k == "ZDOTDIR").then(|| "/z".into()));
        assert!(zsh
            .env
            .contains(&("KODACT_USER_ZDOTDIR".into(), "/z".into())));
        assert!(zsh.env.iter().any(|(k, _)| k == "ZDOTDIR"));

        let sh = dress_in("sh", &[], &d, none);
        assert_eq!(sh.env[0].0, "ENV");
        // An ENV of the person's own stays theirs.
        let theirs = dress_in("sh", &[], &d, |k| (k == "ENV").then(|| "/e".into()));
        assert!(theirs.env.is_empty());

        let ps = dress_in("powershell.exe", &["-NoLogo".into()], &d, none);
        assert_eq!(ps.args[..3], ["-NoLogo", "-NoExit", "-EncodedCommand"]);
        let pwsh = dress_in(
            "C:\\Program Files\\PowerShell\\7\\pwsh.exe",
            &["-NoLogo".into()],
            &d,
            none,
        );
        assert_eq!(pwsh.args.len(), 4);

        let cmd = dress_in("cmd.exe", &[], &d, none);
        assert_eq!(cmd.env, vec![("PROMPT".into(), CMD_PROMPT.into())]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn other_programs_and_own_arguments_are_left_alone() {
        let d = dir("alone");
        for (p, a) in [
            ("fish", vec![]),
            ("ssh", vec!["-t".to_string(), "host".to_string()]),
            ("wsl.exe", vec!["-d".to_string(), "Ubuntu".to_string()]),
            ("bash", vec!["-l".to_string()]),
        ] {
            let out = dress_in(p, &a, &d, none);
            assert_eq!((out.program.as_str(), out.args.clone()), (p, a));
            assert!(out.env.is_empty());
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The real shells, where they are installed: the prompt comes out in
    /// colour, and the person's own startup file still ran.
    #[cfg(unix)]
    #[test]
    fn bash_and_sh_print_a_coloured_prompt() {
        use std::process::Command;
        let d = dir("real");
        let home = d.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".sudo_as_admin_successful"), "").unwrap();
        std::fs::write(home.join(".hushlogin"), "").unwrap();
        // Ubuntu's stock prompt: a window title, no colour.
        std::fs::write(
            home.join(".bashrc"),
            "KODACT_TEST_MINE=yes\nPS1='\\[\\e]0;\\u@\\h: \\w\\a\\]\\u@\\h:\\w\\$ '\n",
        )
        .unwrap();
        if Command::new("bash").arg("--version").output().is_ok() {
            let b = dress_in("bash", &[], &d, none);
            let out = Command::new("bash")
                .args(&b.args)
                .arg("-ic")
                .arg("echo \"$KODACT_TEST_MINE\"; printf %s \"$PS1\"")
                .env("HOME", &home)
                .env_remove("PS1")
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(text.starts_with("yes\n"), "{text}");
            assert!(text.contains("\\e[1;32m"), "{text}");
        }
        let s = dress_in("sh", &[], &d, none);
        let out = Command::new("sh")
            .arg("-ic")
            .arg("printf %s \"$PS1\"")
            .envs(s.env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("\u{1b}[1;32m"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
