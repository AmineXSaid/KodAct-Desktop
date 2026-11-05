# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop: a native app in Rust on GPUI (not web, not a native mobile app). Windows
is the release target; Linux and macOS build too.

## Users

The owner and developers like them: people who work with a coding agent on
their own projects, on Windows, in WSL, over SSH or in a dev container, and who
also use the same app for plain chats and learning (a topic studied over many
chats, notes, diagrams, documents). They work long sessions, often several
chats at once, and watch the agent work rather than only read its answer.

## Product Purpose

KodAct is the desktop app for KodAct CLI (`kodact`), an agent harness that runs on
any model endpoint. Each chat is a KodAct CLI process; the app shows the
conversation, what the agent does (reads, edits, commands, questions,
permissions), and what it makes (documents, diagrams, sheets), and lets you
steer it while it works. Success: a session you can leave running, come back
to, understand at a glance, and trust with your files.

## Positioning

The agent is KodAct CLI on your own endpoint and your own machine (or the remote
one): models are swappable, keys stay on this computer, a remote agent runs
there while the UI and the model calls stay here. Projects carry their own
instructions, knowledge and memory.

## Operating Context

- Long agentic turns: dozens of tool calls, sub-agents, permission prompts,
  questions from the agent, messages sent while it works, Stop and continue.
- Remote work: WSL, SSH hosts, dev containers, with a terminal dock.
- Documents beside the chat: Markdown, diagrams, Word, PowerPoint, Excel, PDF.
- Daily comparison with Claude Desktop, used side by side.

## Capabilities and Constraints

- Views are Rust (GPUI, gpui-component through gpui-kit); every colour, size,
  radius and font comes from `src/theme.rs` tokens (`.interface-design/system.md`
  and `DESIGN.md` are generated from it). Web tooling (CSS detectors, browser
  live mode) does not apply.
- Eleven themes, each in light and dark; fonts are bundled (OFL).
- Dependencies must be Apache-2.0, MIT, BSD, ISC or similar; nothing copied
  from GPL code.
- Terminology: chat, reply, project, folder, endpoint, model, mode (Manual,
  Edit automatically, Plan, Bypass, Expert), permission, question, task,
  side panel, terminal dock.

## Brand Commitments

- Quality reference: Claude Desktop's calm and restraint, with KodAct's own
  identity (its mark, palette and themes), not a copy of Claude.
- Nothing in the current visual identity is binding: the ingot mark, the
  wordmark, the theme set and the home screen's dots may all change when it
  serves the product.
- Voice: plain words, the product's own terms; controls name their action,
  errors name the problem and the way out.

## Evidence on Hand

- Demo mode (`KODACT_DEMO=1`) plays scripted replies for screenshots;
  `scripts/screenshots.sh` and `scripts/e2e/` drive the real app and KodAct CLI.
- No customer quotes, metrics or testimonials exist; none may be invented.

## Product Principles

1. The reply is the hero: everything around it (activity, chrome, menus) is
   quieter than the words.
2. Show what the agent did, truthfully: a failure says so, a stopped call
   says its outcome is unknown, nothing claims work that did not happen.
3. Never lose the user's work or words: questions, queued messages, history.
4. One way to do each thing, reachable from the keyboard and the command
   palette.

## Accessibility & Inclusion

Text at least 4.5:1 against its ground in every theme (`src/palette_check.rs`);
keyboard reachable actions with visible focus; reduced motion honoured
(`KODACT_REDUCED_MOTION=1` or the system setting).
