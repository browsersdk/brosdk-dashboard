use std::path::{Path, PathBuf};

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

pub fn executable_suffix() -> &'static str {
    std::env::consts::EXE_SUFFIX
}
