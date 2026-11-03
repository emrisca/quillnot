// The bundle is a desktop app on every platform we build, so there is no mobile
// entry point to keep here beyond the attribute Tauri's templates carry.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    quillnot_lib::run()
}
