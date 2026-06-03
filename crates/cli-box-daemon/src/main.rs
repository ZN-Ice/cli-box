// crates/cli-box-daemon/src/main.rs
fn main() {
    tracing_subscriber::fmt::init();

    let port = cli_box_core::daemon::find_available_port(15801, 15899)
        .expect("No available port in range 15801-15899");

    tracing::info!("Sandbox daemon started on port {port}");

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(async move { cli_box_core::daemon::run_daemon(port).await })
        .expect("Daemon exited with error");
}
