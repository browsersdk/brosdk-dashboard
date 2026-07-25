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
    #[cfg(all(windows, not(debug_assertions)))]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data).join("BroSDK Dashboard");
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

pub fn secrets_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("secrets")
}

pub fn store_secret(data_dir: &Path, id: &str, secret: &[u8]) -> Result<String, PlatformError> {
    let reference = format!("{id}.bin");
    let directory = secrets_dir(data_dir);
    fs::create_dir_all(&directory)?;
    let protected = protect_secret(secret)?;
    fs::write(directory.join(&reference), protected)?;
    Ok(reference)
}

pub fn read_secret(data_dir: &Path, reference: &str) -> Result<Vec<u8>, PlatformError> {
    let path = safe_secret_path(data_dir, reference)?;
    unprotect_secret(&fs::read(path)?)
}

pub fn delete_secret(data_dir: &Path, reference: &str) -> Result<(), PlatformError> {
    let path = safe_secret_path(data_dir, reference)?;
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

#[cfg(not(windows))]
fn protect_secret(secret: &[u8]) -> Result<Vec<u8>, PlatformError> {
    Ok(secret.to_vec())
}

#[cfg(not(windows))]
fn unprotect_secret(protected: &[u8]) -> Result<Vec<u8>, PlatformError> {
    Ok(protected.to_vec())
}

#[cfg(test)]
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
