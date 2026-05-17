fn main() {
    tauri_build::build();

    #[cfg(target_os = "macos")]
    {
        // Fix @rpath/libswift_Concurrency.dylib crash on macOS 26+
        println!("cargo:rustc-link-arg=-rpath");
        println!("cargo:rustc-link-arg=/usr/lib/swift");
    }
}
