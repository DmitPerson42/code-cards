// Точка входа: скрываем консольное окно в релизной сборке на Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    code_cards_lib::run()
}
