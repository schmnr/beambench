pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_SHA: &str = match option_env!("BB_GIT_SHA") {
    Some(value) => value,
    None => "unknown-git-sha",
};
pub const TARGET_TRIPLE: &str = match option_env!("VERGEN_CARGO_TARGET_TRIPLE") {
    Some(value) => value,
    None => "unknown-target",
};
pub const BUILD_TIMESTAMP: &str = match option_env!("VERGEN_BUILD_TIMESTAMP") {
    Some(value) => value,
    None => "unknown-time",
};
pub const RUSTC_VERSION: &str = match option_env!("VERGEN_RUSTC_SEMVER") {
    Some(value) => value,
    None => "unknown-rustc",
};
