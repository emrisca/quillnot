// The bundle is a desktop app on every platform we build, so there is no mobile
// entry point to keep here beyond the attribute Tauri's templates carry.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--self-test` and `--rank` exercise the live path in the same binary the
    // keychain entry belongs to, then exit. Anything else opens the window. Both
    // flags have to be matched here or the headless mode never runs: `--rank` alone
    // used to fall through and launch the GUI instead.
    if std::env::args().any(|arg| arg == "--self-test" || arg == "--rank") {
        std::process::exit(quillnot_lib::self_test::run());
    }
    quillnot_lib::run()
}
