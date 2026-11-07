//! Embeds the app icon (assets/winsploit.ico, the Iso cube) in winsploit.exe,
//! so Explorer, the taskbar and Task Manager show it.

fn main() {
    println!("cargo:rerun-if-changed=assets/winsploit.rc");
    println!("cargo:rerun-if-changed=assets/winsploit.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/winsploit.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
