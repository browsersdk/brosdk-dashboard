use std::{
    fs,
    path::{Path, PathBuf},
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("platform I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("platform configuration is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("platform secret operation failed: {0}")]
    Secret(String),
}

pub fn platform_id() -> &'static str {
    std::env::consts::OS
}

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("platform crate lives under crates/platform")
        .to_path_buf()
}

pub fn default_sdk_work_dir() -> PathBuf {
    std::env::var_os("BROSDK_WORK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("runtime").join("sdk-work"))
}

pub fn default_data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("BROSDK_DATA_DIR").map(PathBuf::from) {
        return path;
    }
    configured_data_dir().unwrap_or_else(fallback_data_dir)
}

fn fallback_data_dir() -> PathBuf {
    #[cfg(windows)]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data).join("BroSDK Dashboard");
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("BroSDK Dashboard");
    }

    #[cfg(target_os = "linux")]
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(data_home).join("BroSDK Dashboard");
    }
    #[cfg(target_os = "linux")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("BroSDK Dashboard");
    }

    workspace_root().join("runtime").join("data")
}

pub fn platform_config_dir() -> PathBuf {
    #[cfg(windows)]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data)
            .join("BroSDK Dashboard")
            .join("config");
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Preferences")
            .join("BroSDK Dashboard");
    }

    #[cfg(target_os = "linux")]
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config_home).join("BroSDK Dashboard");
    }
    #[cfg(target_os = "linux")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("BroSDK Dashboard");
    }

    workspace_root().join("runtime").join("config")
}

pub fn configured_data_dir() -> Option<PathBuf> {
    let path = platform_config_dir().join("data-dir.json");
    let bytes = fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value
        .get("dataDir")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn set_configured_data_dir(path: &Path) -> Result<(), PlatformError> {
    let config_dir = platform_config_dir();
    fs::create_dir_all(&config_dir)?;
    fs::write(
        config_dir.join("data-dir.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "dataDir": path.display().to_string(),
        }))?,
    )?;
    Ok(())
}

pub fn default_extension_dir() -> PathBuf {
    std::env::var_os("BROSDK_EXTENSION_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_data_dir().join("extensions"))
}

pub fn default_log_dir() -> PathBuf {
    std::env::var_os("BROSDK_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_data_dir().join("logs"))
}

pub fn executable_suffix() -> &'static str {
    std::env::consts::EXE_SUFFIX
}

#[cfg(all(windows, target_arch = "x86_64"))]
const TARGET_TRIPLE: &str = "x86_64-pc-windows-msvc";
#[cfg(all(windows, target_arch = "aarch64"))]
const TARGET_TRIPLE: &str = "aarch64-pc-windows-msvc";
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const TARGET_TRIPLE: &str = "x86_64-apple-darwin";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const TARGET_TRIPLE: &str = "aarch64-apple-darwin";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const TARGET_TRIPLE: &str = "x86_64-unknown-linux-gnu";
#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(windows, target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
)))]
const TARGET_TRIPLE: &str = "unknown-target";

pub fn target_triple() -> &'static str {
    TARGET_TRIPLE
}

#[cfg(all(windows, target_arch = "x86_64"))]
const LIBRARY_DIR_NAME: &str = "windows_x64";
#[cfg(all(windows, target_arch = "aarch64"))]
const LIBRARY_DIR_NAME: &str = "windows_arm64";
#[cfg(target_os = "macos")]
const LIBRARY_DIR_NAME: &str = "macos_universal";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const LIBRARY_DIR_NAME: &str = "linux_x64";
#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(windows, target_arch = "aarch64"),
    target_os = "macos",
    all(target_os = "linux", target_arch = "x86_64")
)))]
const LIBRARY_DIR_NAME: &str = "unsupported";

#[cfg(windows)]
const LIBRARY_FILENAME: &str = "brosdk.dll";
#[cfg(target_os = "macos")]
const LIBRARY_FILENAME: &str = "libbrosdk.dylib";
#[cfg(target_os = "linux")]
const LIBRARY_FILENAME: &str = "libbrosdk.so";
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
const LIBRARY_FILENAME: &str = "brosdk.unsupported";

#[cfg(windows)]
const SECRET_BACKEND: &str = "windows-dpapi";
#[cfg(target_os = "macos")]
const SECRET_BACKEND: &str = "macos-keychain";
#[cfg(target_os = "linux")]
const SECRET_BACKEND: &str = "linux-secret-service";
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
const SECRET_BACKEND: &str = "unsupported";

#[cfg(windows)]
const IPC_TRANSPORT: &str = "named-pipe";
#[cfg(unix)]
const IPC_TRANSPORT: &str = "unix-domain-socket";
#[cfg(not(any(windows, unix)))]
const IPC_TRANSPORT: &str = "unsupported";

pub fn library_dir_name() -> &'static str {
    LIBRARY_DIR_NAME
}

pub fn library_filename() -> &'static str {
    LIBRARY_FILENAME
}

pub fn secret_backend() -> &'static str {
    SECRET_BACKEND
}

pub fn ipc_transport() -> &'static str {
    IPC_TRANSPORT
}

pub fn secrets_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("secrets")
}

pub fn store_secret(data_dir: &Path, id: &str, secret: &[u8]) -> Result<String, PlatformError> {
    let reference = format!("{id}.bin");
    let directory = secrets_dir(data_dir);
    fs::create_dir_all(&directory)?;

    #[cfg(windows)]
    {
        fs::write(directory.join(&reference), protect_secret(secret)?)?;
    }
    #[cfg(unix)]
    {
        let value = std::str::from_utf8(secret)
            .map_err(|error| PlatformError::Secret(format!("secret is not UTF-8: {error}")))?;
        let entry = keyring::Entry::new("com.brosdk.dashboard", &reference)
            .map_err(|error| PlatformError::Secret(error.to_string()))?;
        entry
            .set_password(value)
            .map_err(|error| PlatformError::Secret(error.to_string()))?;
        fs::write(directory.join(&reference), b"keyring:v1")?;
    }
    Ok(reference)
}

pub fn read_secret(data_dir: &Path, reference: &str) -> Result<Vec<u8>, PlatformError> {
    let path = safe_secret_path(data_dir, reference)?;

    #[cfg(windows)]
    {
        return unprotect_secret(&fs::read(path)?);
    }
    #[cfg(unix)]
    {
        let _ = fs::read(path)?;
        let entry = keyring::Entry::new("com.brosdk.dashboard", reference)
            .map_err(|error| PlatformError::Secret(error.to_string()))?;
        return entry
            .get_password()
            .map(|value| value.into_bytes())
            .map_err(|error| PlatformError::Secret(error.to_string()));
    }

    #[allow(unreachable_code)]
    Err(PlatformError::Secret(
        "secret storage is unsupported".into(),
    ))
}

pub fn delete_secret(data_dir: &Path, reference: &str) -> Result<(), PlatformError> {
    let path = safe_secret_path(data_dir, reference)?;

    #[cfg(unix)]
    {
        let entry = keyring::Entry::new("com.brosdk.dashboard", reference)
            .map_err(|error| PlatformError::Secret(error.to_string()))?;
        let _ = entry.delete_credential();
    }

    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn safe_secret_path(data_dir: &Path, reference: &str) -> Result<PathBuf, PlatformError> {
    if reference.is_empty()
        || reference.contains('/')
        || reference.contains('\\')
        || reference.contains("..")
    {
        return Err(PlatformError::Secret("invalid secret reference".into()));
    }
    Ok(secrets_dir(data_dir).join(reference))
}

#[cfg(windows)]
fn protect_secret(secret: &[u8]) -> Result<Vec<u8>, PlatformError> {
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData},
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(PlatformError::Secret(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(bytes)
}

#[cfg(windows)]
fn unprotect_secret(protected: &[u8]) -> Result<Vec<u8>, PlatformError> {
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
        },
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: protected.len() as u32,
        pbData: protected.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(PlatformError::Secret(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(bytes)
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn platform_paths_have_explicit_library_contract() {
        assert!(!library_dir_name().is_empty());
        assert!(!library_filename().is_empty());
        assert!(!target_triple().is_empty());
    }

    #[test]
    fn secret_backend_is_not_plaintext_file_storage() {
        assert_ne!(secret_backend(), "plaintext-file");
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn secret_round_trip_does_not_store_plaintext() {
        let directory =
            std::env::temp_dir().join(format!("brosdk-platform-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let reference =
            store_secret(&directory, "proxy-test", b"top-secret").expect("store secret");
        assert_eq!(
            read_secret(&directory, &reference).expect("read secret"),
            b"top-secret"
        );
        #[cfg(windows)]
        assert_ne!(
            fs::read(secrets_dir(&directory).join(&reference)).expect("protected bytes"),
            b"top-secret"
        );
        delete_secret(&directory, &reference).expect("delete secret");
        let _ = fs::remove_dir_all(directory);
    }
}
