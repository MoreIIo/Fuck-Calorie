// Pas de console noire derrière la fenêtre en version finale sous Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    fc_desktop_lib::run();
}
