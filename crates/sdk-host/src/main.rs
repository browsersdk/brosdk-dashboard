use std::{
    ffi::{c_char, c_void},
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use domain::{
    CallbackCounts, JsonSummary, SdkCapabilities, SmokeReport, SmokeStage, SmokeStageStatus,
    summarize_json,
};
use sdk_ffi::{
    BroSdk, capabilities_for_path, default_env_page_request, default_library_path,
    extract_user_sig, get_user_sig_request, init_request,
};
use serde_json::{Value, json};

static RESULT_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
static LOG_CALLBACKS: AtomicUsize = AtomicUsize::new(0);

struct SmokeReportBuilder {
    skipped: bool,
    started_at: chrono::DateTime<Utc>,
    dll_path: PathBuf,
    work_dir: PathBuf,
    embedded_mcp_port: Option<u16>,
    capabilities: SdkCapabilities,
    stages: Vec<SmokeStage>,
    sdk_info: Option<JsonSummary>,
    env_page: Option<JsonSummary>,
}

impl SmokeReportBuilder {
    fn new() -> Self {
        let dll_path = default_library_path();
        Self {
            skipped: false,
            started_at: Utc::now(),
            capabilities: capabilities_for_path(dll_path.clone()),
            dll_path,
            work_dir: platform::default_sdk_work_dir(),
            embedded_mcp_port: embedded_port(),
            stages: Vec::new(),
            sdk_info: None,
            env_page: None,
        }
    }

    fn finish_after_shutdown(mut self, sdk: &BroSdk) -> SmokeReport {
        self.stages.push(timed_code("sdk_shutdown", || {
            sdk.shutdown().map_err(|err| anyhow!(err))
        }));
        self.finish()
    }

    fn finish(self) -> SmokeReport {
        SmokeReport {
            skipped: self.skipped,
            started_at: self.started_at,
            finished_at: Utc::now(),
            dll_path: self.dll_path.display().to_string(),
            work_dir: Some(self.work_dir.display().to_string()),
            embedded_mcp_port: self.embedded_mcp_port,
            capabilities: self.capabilities,
            stages: self.stages,
            callbacks: CallbackCounts {
                result: RESULT_CALLBACKS.load(Ordering::Relaxed),
                log: LOG_CALLBACKS.load(Ordering::Relaxed),
            },
            sdk_info: self.sdk_info,
            env_page: self.env_page,
        }
    }
}

unsafe extern "C" fn result_callback(
    _code: i32,
    _user_data: *mut c_void,
    _data: *const c_char,
    _len: usize,
) {
    RESULT_CALLBACKS.fetch_add(1, Ordering::Relaxed);
}

unsafe extern "C" fn log_callback(_kind: i32, _data: *const c_char, _len: usize) {
    LOG_CALLBACKS.fetch_add(1, Ordering::Relaxed);
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sdk_host=info".into()),
        )
        .without_time()
        .with_writer(std::io::stderr)
        .init();

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args.first().map(String::as_str).unwrap_or("help");
    let json_output = args.iter().any(|arg| arg == "--json");

    match command {
        "capabilities" => print_json(&capabilities_for_path(default_library_path()), json_output),
        "smoke" => {
            let report = run_smoke();
            print_json(&report, json_output)
        }
        _ => {
            eprintln!("Usage: sdk-host <capabilities|smoke> [--json]");
            Ok(())
        }
    }
}

fn print_json<T: serde::Serialize>(value: &T, _json_output: bool) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn run_smoke() -> SmokeReport {
    RESULT_CALLBACKS.store(0, Ordering::Relaxed);
    LOG_CALLBACKS.store(0, Ordering::Relaxed);

    let mut report = SmokeReportBuilder::new();

    let sdk = match timed("load dll", || {
        BroSdk::load(&report.dll_path).map_err(|err| anyhow!(err))
    }) {
        (stage, Some(sdk)) => {
            report.capabilities = sdk.capabilities();
            report.stages.push(stage);
            sdk
        }
        (stage, None) => {
            report.stages.push(stage);
            return report.finish();
        }
    };

    report.stages.push(timed_code("register log callback", || {
        sdk.register_log_callback(Some(log_callback))
            .map_err(|err| anyhow!(err))
    }));
    report
        .stages
        .push(timed_code("register result callback", || {
            sdk.register_result_callback(Some(result_callback))
                .map_err(|err| anyhow!(err))
        }));

    let api_key = match std::env::var("BROSDK_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            report.skipped = true;
            report.stages.push(SmokeStage {
                name: "BROSDK_API_KEY".into(),
                status: SmokeStageStatus::Skipped,
                code: None,
                message: "environment variable is not set; live SDK stages skipped".into(),
                duration_ms: 0,
            });
            return report.finish_after_shutdown(&sdk);
        }
    };

    report.stages.push(timed_unit("create workDir", || {
        std::fs::create_dir_all(&report.work_dir)
            .with_context(|| format!("failed to create {}", report.work_dir.display()))
    }));

    let user_sig_output = match timed("sdk_get_user_sig", || {
        sdk.get_user_sig(&get_user_sig_request(&api_key))
            .map_err(|err| anyhow!(err))
    }) {
        (stage, Some(output)) => {
            report.stages.push(stage_with_code(stage, output.code));
            output
        }
        (stage, None) => {
            report.stages.push(stage);
            return report.finish_after_shutdown(&sdk);
        }
    };

    let Some(user_sig) = extract_user_sig(&user_sig_output.value) else {
        report.stages.push(SmokeStage {
            name: "extract userSig".into(),
            status: SmokeStageStatus::Failed,
            code: None,
            message: "getUserSig response did not contain data.userSig".into(),
            duration_ms: 0,
        });
        return report.finish_after_shutdown(&sdk);
    };

    let init_body = init_request(user_sig, &report.work_dir, report.embedded_mcp_port);
    match timed("sdk_init", || {
        sdk.init(&init_body).map_err(|err| anyhow!(err))
    }) {
        (stage, Some(output)) => report.stages.push(stage_with_code(stage, output.code)),
        (stage, None) => {
            report.stages.push(stage);
            return report.finish_after_shutdown(&sdk);
        }
    }

    match timed("sdk_info", || sdk.info().map_err(|err| anyhow!(err))) {
        (stage, Some(output)) => {
            report.sdk_info = Some(summarize_json(&output.value));
            report.stages.push(stage_with_code(stage, output.code));
        }
        (stage, None) => report.stages.push(stage),
    }

    let env_page_request = default_env_page_request();
    match timed("sdk_env_page", || {
        sdk.env_page(&env_page_request).map_err(|err| anyhow!(err))
    }) {
        (stage, Some(output)) => {
            report.env_page = Some(summarize_json(&output.value));
            report.stages.push(stage_with_code(stage, output.code));
        }
        (stage, None) => report.stages.push(stage),
    }

    report.finish_after_shutdown(&sdk)
}

fn embedded_port() -> Option<u16> {
    std::env::var("BROSDK_EMBEDDED_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
}

fn timed<T, F>(name: &str, run: F) -> (SmokeStage, Option<T>)
where
    F: FnOnce() -> Result<T>,
{
    let started = Instant::now();
    match run() {
        Ok(value) => (
            SmokeStage {
                name: name.into(),
                status: SmokeStageStatus::Passed,
                code: None,
                message: "ok".into(),
                duration_ms: started.elapsed().as_millis(),
            },
            Some(value),
        ),
        Err(error) => (
            SmokeStage {
                name: name.into(),
                status: SmokeStageStatus::Failed,
                code: None,
                message: redact_error(&error.to_string()),
                duration_ms: started.elapsed().as_millis(),
            },
            None,
        ),
    }
}

fn timed_code<F>(name: &str, run: F) -> SmokeStage
where
    F: FnOnce() -> Result<i32>,
{
    let started = Instant::now();
    match run() {
        Ok(code) => SmokeStage {
            name: name.into(),
            status: SmokeStageStatus::Passed,
            code: Some(code),
            message: "ok".into(),
            duration_ms: started.elapsed().as_millis(),
        },
        Err(error) => SmokeStage {
            name: name.into(),
            status: SmokeStageStatus::Failed,
            code: None,
            message: redact_error(&error.to_string()),
            duration_ms: started.elapsed().as_millis(),
        },
    }
}

fn timed_unit<F>(name: &str, run: F) -> SmokeStage
where
    F: FnOnce() -> Result<()>,
{
    timed_code(name, || {
        run()?;
        Ok(0)
    })
}

fn stage_with_code(mut stage: SmokeStage, code: i32) -> SmokeStage {
    stage.code = Some(code);
    stage
}

fn redact_error(message: &str) -> String {
    let mut value = Value::String(message.to_string());
    sdk_ffi::redact_value(&mut value);
    value
        .as_str()
        .unwrap_or("SDK stage failed")
        .chars()
        .take(512)
        .collect()
}

#[allow(dead_code)]
fn redacted_preview(value: &Value) -> Value {
    let mut value = value.clone();
    sdk_ffi::redact_value(&mut value);
    json!(value)
}
