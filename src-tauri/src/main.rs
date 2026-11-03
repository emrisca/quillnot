// The bundle is a desktop app on every platform we build, so there is no mobile
// entry point to keep here beyond the attribute Tauri's templates carry.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--self-test` exercises the live path in the same binary the keychain entry
    // belongs to, then exits. Anything else opens the window.
    if std::env::args().any(|arg| arg == "--self-test") {
        std::process::exit(quillnot_lib::self_test::run());
    }
    quillnot_lib::run()
}
