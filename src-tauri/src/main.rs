#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if totono_answer_bridge_lib::run_cli_if_requested() {
        return;
    }
    totono_answer_bridge_lib::run();
}
