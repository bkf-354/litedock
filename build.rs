fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").map(|v| v == "windows").unwrap_or(false) {
        embed_resource::compile("res/app.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
