//! A Settings section whose feature waits on ForgeCLI: what it will do, and
//! what it waits on, with no controls, since there's nothing yet for a
//! control to change. The rest of the "Planned" group (Sandbox,
//! Environments, Memory and rules, Plugins) turned out to have engine
//! support and are real editors; Network is what remains.

use gpui::{div, prelude::*, px, relative, IntoElement};

use super::editor_kit as kit;
use super::icon;
use crate::theme::{self, Theme};

pub struct Planned {
    pub title: &'static str,
    pub blurb: &'static str,
    pub will: &'static [&'static str],
    pub waits_on: &'static str,
    /// What already works, and where.
    pub today: &'static str,
}

pub const NETWORK: Planned = Planned {
    title: "Network",
    blurb: "How Forge reaches the internet: through a proxy, with your company's certificates, and which sites it may fetch from.",
    will: &[
        "Send model and web requests through an HTTP or SOCKS proxy, with a bypass list.",
        "Trust extra certificate authorities, for networks that inspect TLS.",
        "Keep a list of sites WebFetch may open without asking, and sites it never may.",
        "Show which endpoint and MCP servers each chat reaches, and how long they take.",
    ],
    waits_on: "ForgeCLI settings for a proxy and extra certificates: today it reads neither, so a Forge setting would change nothing.",
    today: "Allow or deny sites now in Permissions, with rules like WebFetch(domain:example.com). Check an endpoint's DNS, TCP, TLS and key in Models → Diagnose.",
};

pub fn render(t: &Theme, p: &Planned) -> impl IntoElement {
    let mut list = div().flex().flex_col().gap(px(8.));
    for item in p.will {
        list = list.child(
            div()
                .flex()
                .items_start()
                .gap(px(8.))
                .text_size(t.text.body_sm)
                .text_color(t.colors.text)
                .line_height(relative(1.45))
                .child(div().pt(px(3.)).child(icon(
                    "icons/circle-dashed.svg",
                    t.size.icon_xs,
                    t.colors.text_subtle,
                )))
                .child(div().flex_1().min_w_0().child(*item)),
        );
    }
    let note = |label: &str, text: &str, bg| {
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(t.space.md)
            .rounded(t.radius.card)
            .bg(bg)
            .child(
                div()
                    .text_size(t.text.caption)
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.colors.text_strong)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text)
                    .line_height(relative(1.45))
                    .child(text.to_string()),
            )
    };
    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(
            div()
                .flex()
                .items_start()
                .gap(t.space.md)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(kit::header(t, p.title, p.blurb, None)),
                )
                .child(theme::pill(t, t.colors.hover, t.colors.text_muted, "Planned").flex_none()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(kit::group(t, "What it will do"))
                .child(list),
        )
        .child(note("Waits on", p.waits_on, t.extra.warning_tint))
        .child(note("Today", p.today, t.colors.hover))
}
