# LanSend Xcode 27 patch

Baseline: the published `swift-rs` 1.0.8 crate, MIT OR Apache-2.0. Upstream licenses are retained. Source: https://crates.io/crates/swift-rs/1.0.8 and https://github.com/Brendonovich/swift-rs.

Xcode 27 release optimization internalizes `@_cdecl` functions. The upstream globalizer restores each Swift package's own exports but leaves `_retain_object`, `_release_object`, and `_string_from_bytes` local in the embedded `SwiftRs.o` member. Rust's swift-rs bindings then fail to link.

The only code change is in `src-rs/build.rs`: include the `SwiftRs.o` member when globalizing Tauri's root archive, while keeping dependency copies in plugin archives local. This preserves upstream's duplicate-symbol and compiler-helper filters. `llvm-tools` is required. Validate with a full optimized iOS build and archive, not only `cargo check`.

Remove this local patch when upstream provides the equivalent fix.
