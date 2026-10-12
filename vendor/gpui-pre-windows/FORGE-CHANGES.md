# Changes made for forge-gpui

This is `gpui-pre-windows` 0.3.8 (Apache-2.0, `LICENSE-APACHE`), used through
`[patch.crates-io]` in forge-gpui's Cargo.toml. One change:

- `src/directx_renderer.rs`, `build.rs`: the HLSL shaders are embedded with
  `include_str!` and compiled at runtime with `D3DCompile`
  (`d3dcompiler_47.dll`, part of Windows 10 and 11) unless `build.rs` compiled
  them with the Windows SDK's `fxc.exe` (it then sets `gpui_shader_bytes`).
  Upstream compiles them at runtime only in debug builds, from the build
  machine's source path, so a cross-compiled release build could not draw.
