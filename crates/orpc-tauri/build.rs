const COMMANDS: &[&str] = &["handle_rpc_call", "handle_rpc_subscription"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .build();
}
