fn main() {
    // Only apply on macOS
    if cfg!(target_os = "macos") {
        // Add Swift runtime rpath so screencapturekit can find
        // libswift_Concurrency.dylib at runtime
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
