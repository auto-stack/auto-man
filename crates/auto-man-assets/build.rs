//! PLAN-982：让**内嵌资产**成为 cargo 的构建依赖。
//!
//! `auto-man-assets` 用 `rust_embed` 在**编译期**把 `assets/**` 嵌进二进制。
//! cargo 不知道宏读了这些文件，因此在"资产变了但 crate 源码没变"（或两个检出
//! 共享同一个 `CARGO_TARGET_DIR`）时，产物里的资产就是**旧的**——表现为
//! `bytes_frozen_to_baseline_hashes` / `enumeration_keys_and_order_frozen` /
//! `plan_projection_frozen` 这些"冻结基线"用例莫名其妙的红（PLAN-982 复现 4 次）。
//!
//! 这里只做一件事：把 `assets/` 目录声明为重建依赖。这样资产一变，本 crate
//! 必然重编，所有依赖它的测试也拿到一致的嵌入资产。
fn main() {
    println!("cargo:rerun-if-changed=assets");
}
