fn main() {
    // `generate_context!` refuses to compile when the frontend bundle is
    // missing. Rust-only builds (CI's Rust job, `cargo test --workspace`)
    // don't build the frontend, so give them an empty folder; the real app is
    // built through `npm run tauri build`, which fills it first.
    let dist = std::path::Path::new("../dist");
    if !dist.exists() {
        std::fs::create_dir_all(dist).expect("can't create ../dist");
    }
    tauri_build::build();
}
