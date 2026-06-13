fn main() {
    // Only run this on Windows
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        embed_resource::compile("assets/icon.ico", embed_resource::NONE);
    }
}