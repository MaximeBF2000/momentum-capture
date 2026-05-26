fn main() {
    expose_google_env_from_dotenv();
    tauri_build::build();

    // Fix Swift runtime library paths for screencapturekit
    // The screencapturekit crate uses @rpath/libswift_Concurrency.dylib
    // We need to add /usr/lib/swift to the rpath so it can be found
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}

fn expose_google_env_from_dotenv() {
    println!("cargo:rerun-if-changed=../.env");
    println!("cargo:rerun-if-env-changed=GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=GOOGLE_CLIENT_SECRET");

    expose_env_var("GOOGLE_CLIENT_ID");
    expose_env_var("GOOGLE_CLIENT_SECRET");

    if let Ok(content) = std::fs::read_to_string("../.env") {
        expose_dotenv_var(&content, "GOOGLE_CLIENT_ID");
        expose_dotenv_var(&content, "GOOGLE_CLIENT_SECRET");
    }
}

fn expose_env_var(key: &str) {
    if let Ok(value) = std::env::var(key) {
        println!("cargo:rustc-env={}={}", key, value);
    }
}

fn expose_dotenv_var(content: &str, key: &str) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((entry_key, value)) = trimmed.split_once('=') {
            if entry_key.trim() != key || std::env::var(key).is_ok() {
                continue;
            }
            let value = value.trim().trim_matches('"').trim_matches('\'');
            println!("cargo:rustc-env={}={}", key, value);
            break;
        }
    }
}
