# wgpu-ffx (vendored)

AMD FidelityFX FSR 3 upscaler for wgpu, MIT (`LICENSE.MIT`), from https://github.com/cwfitzgerald/wgpu-ffx at
`6e9eb5d` (branch `support-for-wasm-mac`, unreleased: format profiles, Metal through naga), its `wgpu-ffx` and
`wgpu-ffx-shaders-spv` crates only (the SPIR-V is prebuilt upstream). `wgpu30.patch` is what we changed: wgpu 30
(the passthrough descriptor's entry points, `get_mapped_range` returning a `Result`). Excluded from the workspace, so
clippy skips it, and its `rustfmt.toml` keeps upstream's formatting.
