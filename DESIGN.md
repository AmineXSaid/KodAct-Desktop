---
name: KodAct
description: The desktop app for KodAct CLI: long agent sessions in KodAct's ink, paper and lime
colors:
  canvas: "#f6f5f1"
  canvas-dark: "#15181b"
  surface: "#fdfdfb"
  surface-dark: "#1c2024"
  sidebar: "#e9e8e2"
  sidebar-dark: "#0f1113"
  ink-strong: "#08090b"
  ink-strong-dark: "#fbf9f3"
  ink: "#101214"
  ink-dark: "#f2f0e9"
  ink-body: "#2b2e33"
  ink-body-dark: "#d9d7cf"
  ink-muted: "#55595e"
  ink-muted-dark: "#a9adb2"
  ink-subtle: "#63676d"
  ink-subtle-dark: "#868b92"
  border: "rgba(16, 18, 20, 0.13)"
  border-dark: "rgba(226, 230, 233, 0.12)"
  accent: "#b5ea2c"
  accent-dark: "#c6ff3d"
  link: "#4a7200"
  link-dark: "#c6ff3d"
  success: "#2a6f3e"
  success-dark: "#7ccf8f"
  danger: "#b8392e"
  danger-dark: "#ff7a6b"
  warning: "#8f6200"
  warning-dark: "#f0c25e"
typography:
  prose:
    fontFamily: "KodAct Sans"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: 1.533
  ui:
    fontFamily: "KodAct Sans"
    fontSize: "13px"
    fontWeight: 400
  caption:
    fontFamily: "KodAct Sans"
    fontSize: "12px"
    fontWeight: 400
  code:
    fontFamily: "KodAct Mono"
  code-inline:
    fontFamily: "KodAct Mono"
rounded:
  button: "4px"
  pill: "9999px"
  card: "4px"
  field: "4px"
  composer: "4px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
  xxl: "32px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.ink}"
    rounded: "{rounded.pill}"
  user-message:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.card}"
    padding: "10px 16px"
  quiet-line:
    textColor: "{colors.ink-muted}"
    typography: "{typography.ui}"
    height: "26px"
---

# Design: KodAct

> Generated from `src/theme.rs` by `src/system_md.rs`, with `.interface-design/system.md`. Change a token in theme.rs, then run `UPDATE_SYSTEM_MD=1 cargo test system_md`. The product record is `PRODUCT.md`; the theme above is the default one, light and dark, and every other theme keeps the same roles.

## Overview

An Operate surface: an app for long agent sessions, in KodAct CLI's identity: ink, paper and lime, the cursor-and-run tile, the pixel wordmark. The reply is the hero; everything around it (the agent's activity, the chrome, the menus) is quieter than the words. Precise, quick, one brand colour; nothing borrowed from Claude's look.

## Colors

- **Neutrals** carry everything: ink grounds stepping up from the sidebar to the session to cards (paper ones by day), and five inks from strong to subtle; your words a step brighter than KodAct's.
- **Lime** is KodAct acting and what needs you: the primary action and Send (a lime fill under ink), what is live (the mark at work, a waiting dot, the shimmer's peak), links and a switch that is on. On paper, lime that would be a line or a word is olive.
- **Teal and violet** carry structure (modes, skills, memory, code tokens); **peach** marks a name in prose.
- **Status colours** (success, danger, warning) only mean status: a failed call, a stopped one, a change's size.
- Text holds 4.5:1 against its ground in both modes (`src/palette_check.rs`).

## Typography

One sans for interface and prose (KodAct Sans), in regular and semibold; replies at 15px on 23px lines. Code in KodAct Mono; a name in prose (a file, a command) in KodAct Mono, in peach on a faint wash. Small text is sentence case: no uppercase labels and no label above a heading.

## Layout

The transcript and the composer share one 760px column and one edge. Paragraphs 12px apart; tight groups, generous separation. The sidebar is 288px.

## Elevation & Depth

Nearly flat. Lift (`elevation_grain`) only for what holds content you act on: the composer, a card that needs you; popovers a touch more. No glows, no hard offset shadows.

## Shapes

Corners from the tokens above; one radius per role. No coloured edge over 1px on cards or rows: a selected row is a soft fill.

## Components

- **Activity** (tool calls, runs of calls, thinking, answers): quiet lines in the muted ink, 26px high, no icons for what worked; opened, steps indent under a 2px neutral rule. While KodAct works: the mark at work, and the step's words shimmering up to lime.
- **Your message:** a filled bubble without a frame.
- **Cards that need you** (permission, question): a lifted card; the primary action a lime pill under ink, the rest quiet.
- **The mark:** the lime tile with its ink cursor bar and run triangle; beside it in the title bar, "Kod" in the strong ink and "Act" in lime; in About and the opener, the pixel wordmark.
- **Menus and popovers:** one style (radius, padding, row height, shadow) from the tokens.

## Do's and Don'ts

- Do let the reply be the loudest thing on screen.
- Do say what happened in words ("Couldn't write Cargo.toml"), not only with an icon.
- Don't put a kicker or eyebrow above a heading.
- Don't use mono as a technical costume: mono is for code, paths in changes, and data.
- Don't use lime for what isn't KodAct acting, what needs you, a link or a choice that is on.
- Don't dress the window as a terminal: the CLI lends its colours, its mark and its pixel letters, not its function keys or its prompt.
- Don't add motion that isn't a state change; transitions stay within 150 to 250ms.
