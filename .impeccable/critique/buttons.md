# Buttons

Reviewed on Linux screenshots (Xvfb, lavapipe), light and dark: the chat's
permission card, Projects, Settings.

## Fixed

- Filled buttons (primary, accent, danger, Send) were flat patches. They wear
  `Theme::button_face` (a shade lighter at the top, deeper at the foot) and
  sit on `elevation_button`; pressed, they go flat and a shade deeper.
- Bordered buttons (Cancel, Always allow) sit on the same hairline lift.
- Both routes to a button (`ui::button::Button`, `theme::*_button`) share it,
  so there is still one button system. The sidebar's and the rail's own
  buttons are as they were.
- Controls that never worked are gone rather than greyed: Re-apply on a
  reverted change, the model capability check in Diagnostics. Copy as JSON
  no longer promises share links.

## Left

- P3: ghost and text buttons have no press state beyond a dimmed fill.
