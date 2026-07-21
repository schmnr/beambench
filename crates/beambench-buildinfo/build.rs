use std::process::Command;

fn main() {
    if let Err(err) = emit_vergen() {
        println!("cargo:warning=vergen build metadata unavailable: {err}");
        println!("cargo:rustc-env=VERGEN_CARGO_TARGET_TRIPLE=unknown-target");
        println!("cargo:rustc-env=VERGEN_BUILD_TIMESTAMP=unknown-time");
        println!("cargo:rustc-env=VERGEN_RUSTC_SEMVER=unknown-rustc");
    }

    let git_sha = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|sha| sha.trim().to_owned())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown-git-sha".to_owned());

    println!("cargo:rustc-env=BB_GIT_SHA={git_sha}");
}

fn emit_vergen() -> Result<(), Box<dyn std::error::Error>> {
    use vergen::{BuildBuilder, CargoBuilder, Emitter, RustcBuilder};

    let build = BuildBuilder::default().build_timestamp(true).build()?;
    let cargo = CargoBuilder::default().target_triple(true).build()?;
    let rustc = RustcBuilder::default().semver(true).build()?;

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&cargo)?
        .add_instructions(&rustc)?
        .emit()?;

    Ok(())
}
