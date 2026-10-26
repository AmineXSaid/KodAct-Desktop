---
name: Forge
description: The desktop app for ForgeCLI: a calm workspace for long agent sessions
colors:
  canvas: "#f7f5f1"
  canvas-dark: "#242322"
  surface: "#ffffff"
  surface-dark: "#2e2d2b"
  sidebar: "#ebe8e1"
  sidebar-dark: "#1b1a19"
  ink-strong: "#1e1e1e"
  ink-strong-dark: "#f5f2f0"
  ink: "#1e1e1e"
  ink-dark: "#f0e9e5"
  ink-body: "#2b2825"
  ink-body-dark: "#d9d1cb"
  ink-muted: "#5a554f"
  ink-muted-dark: "#a8a29b"
  ink-subtle: "#7d776f"
  ink-subtle-dark: "#857f77"
  border: "rgba(30, 30, 30, 0.13)"
  border-dark: "rgba(242, 233, 227, 0.12)"
  accent: "#b4532a"
  accent-dark: "#e2895b"
  link: "#a44a24"
  link-dark: "#eb9d74"
  success: "#3f6b4b"
  success-dark: "#8fbf9b"
  danger: "#a3402c"
  danger-dark: "#e98a72"
  warning: "#8a5600"
  warning-dark: "#ffc85a"
typography:
  prose:
    fontFamily: "Archivo"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: 1.533
  ui:
    fontFamily: "Archivo"
    fontSize: "13px"
    fontWeight: 400
  caption:
    fontFamily: "Archivo"
    fontSize: "12px"
    fontWeight: 400
  code:
    fontFamily: "JetBrains Mono"
  code-inline:
    fontFamily: "IBM Plex Mono"
rounded:
  button: "4px"
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
    rounded: "{rounded.button}"
  user-message:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.card}"
    padding: "10px 16px"
  quiet-line:
    textColor: "{colors.ink-muted}"
    typography: "{typography.ui}"
    height: "26px"
---

# Design: Forge

> Generated from `src/theme.rs` by `src/system_md.rs`, with `.interface-design/system.md`. Change a token in theme.rs, then run `UPDATE_SYSTEM_MD=1 cargo test system_md`. The product record is `PRODUCT.md`; the theme above is the default one, light and dark, and every other theme keeps the same roles.

## Overview

An Operate surface: an app for long agent sessions, judged against Claude Desktop's calm. The reply is the hero; everything around it (the agent's activity, the chrome, the menus) is quieter than the words. Restraint, precision and one accent, not decoration.

## Colors

- **Neutrals** carry everything: a canvas, a slightly different sidebar layer, a surface for the few things that hold content (your messages, an opened change, a card that needs you), and five inks from strong to subtle.
- **The accent** is only for the primary action, the current selection and state (a waiting dot, a switch that is on). Never for decoration or an inactive control.
- **Status colours** (success, danger, warning) only mean status: a failed call, a stopped one, a change's size.
- Text holds 4.5:1 against its ground in every theme (`src/palette_check.rs`).

## Typography

One sans for interface and prose (Archivo), in regular and semibold; replies at 15px on 23px lines. Code in JetBrains Mono; a name in prose (a file, a command) in IBM Plex Mono on a faint neutral wash, never coloured. Small text is sentence case: no uppercase labels and no label above a heading.

## Layout

The transcript and the composer share one 840px column and one edge. Paragraphs 12px apart; tight groups, generous separation. The sidebar is 288px.

## Elevation & Depth

Nearly flat. Lift (`elevation_grain`) only for what holds content you act on: the composer, a card that needs you; popovers a touch more. No glows, no hard offset shadows.

## Shapes

Corners from the tokens above; one radius per role. No coloured edge over 1px on cards or rows: a selected row is a soft fill.

## Components

- **Activity** (tool calls, runs of calls, thinking, answers): quiet lines in the muted ink, 26px high, no icons for what worked; opened, steps indent under a 2px neutral rule.
- **Your message:** a filled bubble without a frame.
- **Cards that need you** (permission, question): a lifted card; the primary action in the accent, the rest quiet.
- **Menus and popovers:** one style (radius, padding, row height, shadow) from the tokens.

## Do's and Don'ts

- Do let the reply be the loudest thing on screen.
- Do say what happened in words ("Couldn't write Cargo.toml"), not only with an icon.
- Don't put a kicker or eyebrow above a heading.
- Don't use mono as a technical costume: mono is for code, paths in changes, and data.
- Don't use colour for what isn't primary, selected or a state.
- Don't add motion that isn't a state change; transitions stay within 150 to 250ms.
