// Windows release builds must not open a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Started again by itself to decode a DWG file out of process.
    if let Some(code) = carve_core::io::dwg::run_helper(std::env::args()) {
        std::process::exit(code);
    }
    carve_core::io::dwg::set_helper(carve_core::io::dwg::Helper::current_exe());
    carvestudio_lib::run()
}
