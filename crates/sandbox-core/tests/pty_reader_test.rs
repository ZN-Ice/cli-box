#![cfg(target_os = "macos")]

use sandbox_core::process::ProcessManager;

#[test]
fn test_pty_reader_thread_basic() {
    // Spawn a simple command that echoes input
    let info = ProcessManager::spawn_cli("cat", &[]).expect("Failed to spawn cat");
    eprintln!("Spawned cat: pid={}", info.pid);

    // Wait for reader thread to start
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Send input
    ProcessManager::send_input(info.pid, b"hello\r").expect("Failed to send input");
    eprintln!("Sent input");

    // Wait for output to appear in buffer
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Try reading multiple times
    let mut got_data = false;
    for i in 0..10 {
        match ProcessManager::read_output(info.pid) {
            Ok(Some(data)) => {
                eprintln!(
                    "Read {}: {} chars: {:?}",
                    i,
                    data.len(),
                    &data[..data.len().min(100)]
                );
                got_data = true;
                break;
            }
            Ok(None) => {
                eprintln!("Read {}: None", i);
            }
            Err(e) => {
                eprintln!("Read {}: Error: {}", i, e);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    // Cleanup
    ProcessManager::kill_process(info.pid).expect("Failed to kill");

    assert!(got_data, "Expected to read data from PTY but got nothing");
}

#[test]
fn test_pty_reader_opencode() {
    let info = ProcessManager::spawn_cli("opencode", &[]).expect("Failed to spawn opencode");
    eprintln!("Spawned opencode: pid={}", info.pid);

    // Wait for opencode to start
    std::thread::sleep(std::time::Duration::from_secs(5));

    // Send input
    ProcessManager::send_input(info.pid, b"hello\r").expect("Failed to send input");
    eprintln!("Sent input");

    // Wait and read
    std::thread::sleep(std::time::Duration::from_secs(3));

    let mut total = String::new();
    for i in 0..10 {
        match ProcessManager::read_output(info.pid) {
            Ok(Some(data)) => {
                eprintln!("Read {}: {} chars", i, data.len());
                total.push_str(&data);
            }
            Ok(None) => {
                eprintln!("Read {}: None", i);
            }
            Err(e) => {
                eprintln!("Read {}: Error: {}", i, e);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    eprintln!("Total chars: {}", total.len());
    ProcessManager::kill_process(info.pid).expect("Failed to kill");

    assert!(
        !total.is_empty(),
        "Expected opencode to produce output but got nothing"
    );
}

#[test]
fn test_pty_reader_opencode_with_resize() {
    let info = ProcessManager::spawn_cli("opencode", &[]).expect("Failed to spawn opencode");
    eprintln!("Spawned opencode: pid={}", info.pid);

    // Wait for opencode to start
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Resize PTY (simulating what the frontend does)
    ProcessManager::resize_pty(info.pid, 120, 40).expect("Failed to resize");
    eprintln!("Resized PTY to 120x40");

    std::thread::sleep(std::time::Duration::from_secs(2));

    // Send input
    ProcessManager::send_input(info.pid, b"hello\r").expect("Failed to send input");
    eprintln!("Sent input");

    // Wait and read
    std::thread::sleep(std::time::Duration::from_secs(3));

    let mut total = String::new();
    for i in 0..10 {
        match ProcessManager::read_output(info.pid) {
            Ok(Some(data)) => {
                eprintln!("Read {}: {} chars", i, data.len());
                total.push_str(&data);
            }
            Ok(None) => {
                eprintln!("Read {}: None", i);
            }
            Err(e) => {
                eprintln!("Read {}: Error: {}", i, e);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    eprintln!("Total chars: {}", total.len());
    ProcessManager::kill_process(info.pid).expect("Failed to kill");

    assert!(
        !total.is_empty(),
        "Expected opencode to produce output after resize but got nothing"
    );
}
