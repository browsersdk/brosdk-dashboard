use std::{
    ffi::{CStr, c_char, c_void},
    path::{Path, PathBuf},
    ptr,
};

use domain::SdkCapabilities;
use libloading::Library;
use serde_json::{Value, json};
use thiserror::Error;

pub type SdkHandle = *mut c_void;
pub type SdkResultCallback =
    unsafe extern "C" fn(code: i32, user_data: *mut c_void, data: *const c_char, len: usize);
pub type SdkLogCallback = unsafe extern "C" fn(kind: i32, data: *const c_char, len: usize);

type RegisterResultCb = unsafe extern "C" fn(Option<SdkResultCallback>, *mut c_void) -> i32;
type RegisterLogCb = unsafe extern "C" fn(Option<SdkLogCallback>) -> i32;
type JsonOutCall = unsafe extern "C" fn(*const c_char, usize, *mut *mut c_char, *mut usize) -> i32;
type InitCall =
    unsafe extern "C" fn(*mut SdkHandle, *const c_char, usize, *mut *mut c_char, *mut usize) -> i32;
type InfoCall = unsafe extern "C" fn(*mut *mut c_char, *mut usize) -> i32;
type AsyncJsonCall = unsafe extern "C" fn(*const c_char, usize) -> i32;
type ShutdownCall = unsafe extern "C" fn() -> i32;
type FreeCall = unsafe extern "C" fn(*mut c_void);
type StaticStringCall = unsafe extern "C" fn(i32) -> *const c_char;

#[derive(Debug, Error)]
pub enum SdkFfiError {
    #[error("brosdk.dll not found at {0}")]
    DllNotFound(PathBuf),
    #[error("failed to load {path}: {source}")]
    Load {
        path: PathBuf,
        source: libloading::Error,
    },
    #[error("required SDK symbol is missing: {0}")]
    MissingSymbol(String),
    #[error("{function} returned {code}: {message}")]
    Call {
        function: &'static str,
        code: i32,
        message: String,
        output: Option<Value>,
    },
    #[error("{function} returned invalid UTF-8")]
    Utf8 { function: &'static str },
    #[error("{function} returned non-JSON output")]
    Json {
        function: &'static str,
        output: String,
    },
}

#[derive(Debug, Clone)]
pub struct SdkCallOutput {
    pub code: i32,
    pub value: Value,
    pub raw_len: usize,
}

pub struct BroSdk {
    _library: Library,
    path: PathBuf,
    register_result_cb: RegisterResultCb,
    register_log_cb: RegisterLogCb,
    get_user_sig: JsonOutCall,
    init: InitCall,
    info: InfoCall,
    env_page: JsonOutCall,
    env_get_info: JsonOutCall,
    network_diagnostics: JsonOutCall,
    system_proxy_diagnostics: InfoCall,
    browser_install: AsyncJsonCall,
    browser_cleanup: JsonOutCall,
    browser_info: InfoCall,
    browser_command: JsonOutCall,
    browser_snapshot: JsonOutCall,
    browser_env_check: JsonOutCall,
    browser_open: AsyncJsonCall,
    browser_close: AsyncJsonCall,
    shutdown: ShutdownCall,
    free: FreeCall,
    error_name: Option<StaticStringCall>,
    error_string: Option<StaticStringCall>,
}

impl BroSdk {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SdkFfiError> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(SdkFfiError::DllNotFound(path));
        }

        // SAFETY: Loading is scoped to this host process. Function pointers are copied
        // only after resolving symbols from the same live Library value.
        let library = unsafe { Library::new(&path) }.map_err(|source| SdkFfiError::Load {
            path: path.clone(),
            source,
        })?;

        // SAFETY: Required symbols match the public C ABI in libs/windows_x64/brosdk.h.
        let sdk = unsafe {
            Self {
                register_result_cb: required(&library, b"sdk_register_result_cb\0")?,
                register_log_cb: required(&library, b"sdk_register_log_cb\0")?,
                get_user_sig: required(&library, b"sdk_get_user_sig\0")?,
                init: required(&library, b"sdk_init\0")?,
                info: required(&library, b"sdk_info\0")?,
                env_page: required(&library, b"sdk_env_page\0")?,
                env_get_info: required(&library, b"sdk_env_getinfo\0")?,
                network_diagnostics: required(&library, b"sdk_network_diagnostics\0")?,
                system_proxy_diagnostics: required(&library, b"sdk_system_proxy_diagnostics\0")?,
                browser_install: required(&library, b"sdk_browser_install\0")?,
                browser_cleanup: required(&library, b"sdk_browser_cleanup\0")?,
                browser_info: required(&library, b"sdk_browser_info\0")?,
                browser_command: required(&library, b"sdk_browser_command\0")?,
                browser_snapshot: required(&library, b"sdk_browser_snapshot\0")?,
                browser_env_check: required(&library, b"sdk_browser_env_check\0")?,
                browser_open: required(&library, b"sdk_browser_open\0")?,
                browser_close: required(&library, b"sdk_browser_close\0")?,
                shutdown: required(&library, b"sdk_shutdown\0")?,
                free: required(&library, b"sdk_free\0")?,
                error_name: optional(&library, b"sdk_error_name\0"),
                error_string: optional(&library, b"sdk_error_string\0"),
                _library: library,
                path,
            }
        };

        Ok(sdk)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn capabilities(&self) -> SdkCapabilities {
        capabilities_for_path(self.path.clone())
    }

    pub fn register_result_callback(
        &self,
        cb: Option<SdkResultCallback>,
    ) -> Result<i32, SdkFfiError> {
        let code = unsafe { (self.register_result_cb)(cb, ptr::null_mut()) };
        self.check_code("sdk_register_result_cb", code, None)?;
        Ok(code)
    }

    pub fn register_log_callback(&self, cb: Option<SdkLogCallback>) -> Result<i32, SdkFfiError> {
        let code = unsafe { (self.register_log_cb)(cb) };
        self.check_code("sdk_register_log_cb", code, None)?;
        Ok(code)
    }

    pub fn get_user_sig(&self, request: &Value) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_json_out("sdk_get_user_sig", self.get_user_sig, request)
    }

    pub fn init(&self, request: &Value) -> Result<SdkCallOutput, SdkFfiError> {
        let bytes = serde_json::to_vec(request).expect("JSON value always serializes");
        let mut handle: SdkHandle = ptr::null_mut();
        let mut out = ptr::null_mut();
        let mut out_len = 0usize;
        let code = unsafe {
            (self.init)(
                &mut handle,
                bytes.as_ptr().cast::<c_char>(),
                bytes.len(),
                &mut out,
                &mut out_len,
            )
        };
        let value = self.take_json_output("sdk_init", out, out_len)?;
        self.check_code("sdk_init", code, Some(value.clone()))?;
        Ok(SdkCallOutput {
            code,
            value,
            raw_len: out_len,
        })
    }

    pub fn info(&self) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_info("sdk_info", self.info)
    }

    pub fn env_page(&self, request: &Value) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_json_out("sdk_env_page", self.env_page, request)
    }

    pub fn env_get_info(&self, request: &Value) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_json_out("sdk_env_getinfo", self.env_get_info, request)
    }

    pub fn network_diagnostics(&self, request: &Value) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_json_out("sdk_network_diagnostics", self.network_diagnostics, request)
    }

    pub fn system_proxy_diagnostics(&self) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_info(
            "sdk_system_proxy_diagnostics",
            self.system_proxy_diagnostics,
        )
    }

    pub fn browser_install(&self, request: &Value) -> Result<i32, SdkFfiError> {
        self.call_async_json("sdk_browser_install", self.browser_install, request)
    }

    pub fn browser_cleanup(&self, request: &Value) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_json_out("sdk_browser_cleanup", self.browser_cleanup, request)
    }

    pub fn browser_info(&self) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_info("sdk_browser_info", self.browser_info)
    }

    pub fn browser_command(&self, request: &Value) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_json_out("sdk_browser_command", self.browser_command, request)
    }

    pub fn browser_snapshot(&self, request: &Value) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_json_out("sdk_browser_snapshot", self.browser_snapshot, request)
    }

    pub fn browser_env_check(&self, request: &Value) -> Result<SdkCallOutput, SdkFfiError> {
        self.call_json_out("sdk_browser_env_check", self.browser_env_check, request)
    }

    pub fn browser_open(&self, request: &Value) -> Result<i32, SdkFfiError> {
        self.call_async_json("sdk_browser_open", self.browser_open, request)
    }

    pub fn browser_close(&self, request: &Value) -> Result<i32, SdkFfiError> {
        self.call_async_json("sdk_browser_close", self.browser_close, request)
    }

    pub fn shutdown(&self) -> Result<i32, SdkFfiError> {
        let code = unsafe { (self.shutdown)() };
        self.check_code("sdk_shutdown", code, None)?;
        Ok(code)
    }

    fn call_json_out(
        &self,
        function: &'static str,
        call: JsonOutCall,
        request: &Value,
    ) -> Result<SdkCallOutput, SdkFfiError> {
        let bytes = serde_json::to_vec(request).expect("JSON value always serializes");
        let mut out = ptr::null_mut();
        let mut out_len = 0usize;
        let code = unsafe {
            call(
                bytes.as_ptr().cast::<c_char>(),
                bytes.len(),
                &mut out,
                &mut out_len,
            )
        };
        let value = self.take_json_output(function, out, out_len)?;
        self.check_code(function, code, Some(value.clone()))?;
        Ok(SdkCallOutput {
            code,
            value,
            raw_len: out_len,
        })
    }

    fn call_info(
        &self,
        function: &'static str,
        call: InfoCall,
    ) -> Result<SdkCallOutput, SdkFfiError> {
        let mut out = ptr::null_mut();
        let mut out_len = 0usize;
        let code = unsafe { call(&mut out, &mut out_len) };
        let value = self.take_json_output(function, out, out_len)?;
        self.check_code(function, code, Some(value.clone()))?;
        Ok(SdkCallOutput {
            code,
            value,
            raw_len: out_len,
        })
    }

    fn call_async_json(
        &self,
        function: &'static str,
        call: AsyncJsonCall,
        request: &Value,
    ) -> Result<i32, SdkFfiError> {
        let bytes = serde_json::to_vec(request).expect("JSON value always serializes");
        let code = unsafe { call(bytes.as_ptr().cast::<c_char>(), bytes.len()) };
        self.check_code(function, code, None)?;
        Ok(code)
    }

    fn take_json_output(
        &self,
        function: &'static str,
        out: *mut c_char,
        out_len: usize,
    ) -> Result<Value, SdkFfiError> {
        if out.is_null() || out_len == 0 {
            return Ok(Value::Null);
        }

        let bytes = unsafe { std::slice::from_raw_parts(out.cast::<u8>(), out_len) }.to_vec();
        unsafe { (self.free)(out.cast::<c_void>()) };

        let text = String::from_utf8(bytes).map_err(|_| SdkFfiError::Utf8 { function })?;
        serde_json::from_str(&text).map_err(|_| SdkFfiError::Json {
            function,
            output: redact_text(&text),
        })
    }

    fn check_code(
        &self,
        function: &'static str,
        code: i32,
        output: Option<Value>,
    ) -> Result<(), SdkFfiError> {
        if code >= 0 {
            return Ok(());
        }
        let mut redacted = output;
        if let Some(value) = redacted.as_mut() {
            redact_value(value);
        }
        Err(SdkFfiError::Call {
            function,
            code,
            message: self.error_message(code),
            output: redacted,
        })
    }

    fn error_message(&self, code: i32) -> String {
        let detail = self
            .error_string
            .and_then(|call| unsafe { read_static_string(call(code)) });
        let name = self
            .error_name
            .and_then(|call| unsafe { read_static_string(call(code)) });
        match (name, detail) {
            (Some(name), Some(detail)) if name != detail => format!("{name}: {detail}"),
            (Some(name), _) => name,
            (_, Some(detail)) => detail,
            _ => "SDK call failed".into(),
        }
    }
}

pub fn default_library_path() -> PathBuf {
    std::env::var_os("BROSDK_DLL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .expect("sdk-ffi crate lives under crates/sdk-ffi")
                .join("libs")
                .join("windows_x64")
                .join("brosdk.dll")
        })
}

pub fn capabilities_for_path(path: impl Into<PathBuf>) -> SdkCapabilities {
    let path = path.into();
    SdkCapabilities {
        dll_path: Some(path.display().to_string()),
        dll_exists: path.exists(),
        ..SdkCapabilities::default()
    }
}

pub fn get_user_sig_request(api_key: &str) -> Value {
    json!({
        "apiKey": api_key,
        "role": "user"
    })
}

pub fn init_request(
    user_sig: &str,
    work_dir: &Path,
    embedded_port: Option<u16>,
    sdk_api_url: Option<&str>,
    debug: bool,
) -> Value {
    let mut value = json!({
        "userSig": user_sig,
        "workDir": work_dir.display().to_string(),
        "debug": debug
    });
    if let Some(port) = embedded_port {
        value["port"] = json!(port);
    }
    if let Some(sdk_api_url) = sdk_api_url.filter(|value| !value.trim().is_empty()) {
        value["sdkApiUrl"] = json!(sdk_api_url);
    }
    value
}

pub fn default_env_page_request() -> Value {
    std::env::var("BROSDK_ENV_PAGE_REQUEST")
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({ "page": 1, "pageSize": 20 }))
}

pub fn extract_user_sig(value: &Value) -> Option<&str> {
    value
        .pointer("/data/userSig")
        .and_then(Value::as_str)
        .or_else(|| value.get("userSig").and_then(Value::as_str))
}

pub fn redact_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if is_sensitive_key(key) {
                    *child = Value::String("[redacted]".into());
                } else {
                    redact_value(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_value(item);
            }
        }
        Value::String(text) => {
            if looks_like_sensitive_text(text) {
                *text = "[redacted]".into();
            } else if let Some(redacted) = redact_url_credentials(text) {
                *text = redacted;
            }
        }
        _ => {}
    }
}

fn redact_text(text: &str) -> String {
    let mut value = Value::String(text.to_string());
    redact_value(&mut value);
    value
        .as_str()
        .unwrap_or("[redacted]")
        .chars()
        .take(512)
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "apikey",
        "api_key",
        "usersig",
        "user_sig",
        "authorization",
        "cookie",
        "cookies",
        "password",
        "passwd",
        "token",
        "secret",
        "appsecret",
        "accesskeysecret",
        "securitytoken",
        "cdk",
        "dek",
        "seed",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn looks_like_sensitive_text(text: &str) -> bool {
    text.starts_with("sk-") || text.contains("Bearer ") || text.len() > 120 && text.contains('.')
}

fn redact_url_credentials(text: &str) -> Option<String> {
    let scheme_end = text.find("://")?;
    let authority_start = scheme_end + 3;
    let authority_end = text[authority_start..]
        .find(['/', '?', '#'])
        .map(|index| authority_start + index)
        .unwrap_or(text.len());
    let authority = &text[authority_start..authority_end];
    let at = authority.rfind('@')?;
    let credentials = &authority[..at];
    let colon = credentials.find(':')?;
    let username = &credentials[..colon];
    Some(format!(
        "{}{}:***@{}{}",
        &text[..authority_start],
        username,
        &authority[at + 1..],
        &text[authority_end..]
    ))
}

unsafe fn required<T: Copy>(library: &Library, name: &'static [u8]) -> Result<T, SdkFfiError> {
    let symbol = unsafe { library.get::<T>(name) }.map_err(|_| {
        SdkFfiError::MissingSymbol(String::from_utf8_lossy(&name[..name.len() - 1]).into())
    })?;
    Ok(*symbol)
}

unsafe fn optional<T: Copy>(library: &Library, name: &'static [u8]) -> Option<T> {
    unsafe { library.get::<T>(name) }.ok().map(|symbol| *symbol)
}

unsafe fn read_static_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capabilities_include_embedded_mcp() {
        let caps = capabilities_for_path(default_library_path());
        assert!(caps.c_abi);
        assert!(caps.embedded_web_api);
        assert!(caps.embedded_mcp);
        assert!(caps.supports_init_port);
    }

    #[test]
    fn redacts_nested_sensitive_fields() {
        let mut value = json!({
            "data": {
                "userSig": "abc",
                "nested": [{ "proxyPassword": "123" }]
            }
        });
        redact_value(&mut value);
        assert_eq!(value["data"]["userSig"], "[redacted]");
        assert_eq!(value["data"]["nested"][0]["proxyPassword"], "[redacted]");
    }

    #[test]
    fn redacts_password_embedded_in_proxy_url() {
        let mut value = json!({ "proxy": "socks5://alice:secret@127.0.0.1:1080" });
        redact_value(&mut value);
        assert_eq!(value["proxy"], "socks5://alice:***@127.0.0.1:1080");
    }
}
