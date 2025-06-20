use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    string::String,
};

use anyhow::{Context, bail};
use bpf_features::BpfFeatures;

static CLANG_DEFAULT: &str = "clang";
static LLVM_STRIP: &str = "llvm-strip";
static INCLUDE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/include");

// Given a probe name and the eBPF program source code path, compile it to OUT_DIR.
// We'll build multiple versions with all combinations of eBPF features we rely
// on. Lack of certain feature will result in using legacy replacements.
pub fn build(name: &str, source: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-changed={INCLUDE_PATH}");

    let out_dir = env::var("OUT_DIR").context("OUT_DIR not set")?;
    let out_path = Path::new(&out_dir).join(name);

    for (_, (bpf_objfile_suffix, build_args)) in BpfFeatures::all_combinations() {
        let filename = format!("{name}.{bpf_objfile_suffix}");
        let full_path = out_path.with_file_name(filename);
        compile(source, full_path, &build_args)
            .context("Error compiling programs with features: {build_args:?}")?;
    }

    Ok(())
}

fn compile(probe: &str, out_object: PathBuf, extra_args: &[String]) -> anyhow::Result<()> {
    let clang = env::var("CLANG").unwrap_or_else(|_| String::from(CLANG_DEFAULT));
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let include_path = PathBuf::from(INCLUDE_PATH);
    let status = Command::new(clang)
        .arg(format!("-I{}", include_path.to_string_lossy()))
        .arg(format!("-I{}", include_path.join(&arch).to_string_lossy()))
        .arg("-g")
        .arg("-O2")
        .args(["-target", "bpf"])
        .arg("-c")
        .arg("-Werror")
        .arg("-fno-stack-protector")
        .arg(format!(
            "-D__TARGET_ARCH_{}",
            match arch.as_str() {
                "x86_64" => "x86".to_string(),
                "aarch64" => "arm64".to_string(),
                "riscv64" => "riscv".to_string(),
                _ => arch.clone(),
            }
        ))
        .args(extra_args)
        .arg(probe)
        .arg("-o")
        .arg(&out_object)
        .status()
        .context("Failed to execute clang")?;

    if !status.success() {
        bail!("Failed to compile eBPF program");
    }

    // Strip debug symbols
    let status = Command::new(LLVM_STRIP)
        .arg("-g")
        .arg(out_object)
        .status()
        .context("Failed to execute llvm-strip")?;

    if !status.success() {
        bail!("Failed strip eBPF program");
    }

    Ok(())
}
