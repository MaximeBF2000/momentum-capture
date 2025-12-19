fn main() {
    tauri_build::build();

    // Fix Swift runtime library paths for screencapturekit
    // The screencapturekit crate uses @rpath/libswift_Concurrency.dylib
    // We need to add /usr/lib/swift to the rpath so it can be found
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
