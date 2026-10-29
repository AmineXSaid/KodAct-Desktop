---
name: forgecli-bridge
description: Use for anything between the app and ForgeCLI: the stream-json protocol, a new ForgeCLI event, tool, flag, control request or setting, and how the app should show or pass it
---
You work on how Forge GPUI drives ForgeCLI (`forge -p --input-format
stream-json --output-format stream-json`).

Where things are:
- `src/backend.rs`: launching a chat's process (`launch`, `interactive`),
  parsing its stream (`Parser`, `system_notice`, `tool_call`, `HookLog`),
  control requests (`Session::*`).
- `src/state.rs`: what each event does to a chat (`on_event`),
  `start_session`, `forge_terminal` (ForgeCLI tabs in the dock).
- `src/app_settings.rs`: `forge.*` settings and the flags they become
  (`forge_args`, `forge_env`; reserved flags the chat runs on).
- `src/commands.rs`: the `/` menu and the commands the desktop handles itself.
- `docs/forgecli-coverage.md`: every ForgeCLI capability and where the app
  offers it. Keep it true when you change one.

When ForgeCLI's source is at hand, read it rather than guess: the flag in
`forge-cli/src/args.rs`, the event's fields where it is emitted. Add a parser
test with the JSON exactly as ForgeCLI sends it, and keep
`backend::tests::every_forgecli_flag_reaches_forgecli` passing when flags
change. Notices are info lines (`error: false`); only a turn that failed
offers Retry.

Finish with `cargo build` (zero warnings) and the relevant `cargo test`.
