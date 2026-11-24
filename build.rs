use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let assets = [
        "assets/W10Wheel.rc",
        "assets/W10Wheel.manifest",
        "assets/AppIcon.ico",
        "assets/TrayIcon-Run.ico",
        "assets/TrayIcon-Stop.ico",
    ];
    for asset in assets {
        println!("cargo:rerun-if-changed={asset}");
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let assets_dir = manifest_dir.join("assets");
    let res_path = out_dir.join("W10Wheel.res");

    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default().as_str() {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        other => panic!("unsupported Windows target arch for rc.exe: {other}"),
    };

    let rc_exe = find_rc_exe(arch).expect("could not locate Windows SDK rc.exe");
    let status = Command::new(&rc_exe)
        .current_dir(&assets_dir)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_path)
        .arg("W10Wheel.rc")
        .status()
        .expect("failed to launch rc.exe");
    assert!(status.success(), "rc.exe failed with status {status}");

    println!("cargo:rustc-link-arg-bin=W10Wheel={}", res_path.display());
}

fn find_rc_exe(arch: &str) -> Option<PathBuf> {
    if let Ok(status) = Command::new("rc.exe").arg("/?").output()
        && status.status.success()
    {
        return Some(PathBuf::from("rc.exe"));
    }

    let mut kit_roots = Vec::new();
    if let Some(sdk) = env::var_os("WindowsSdkDir") {
        let bin = PathBuf::from(sdk).join("bin");
        kit_roots.push(bin);
    }
    if let Some(pf86) = env::var_os("ProgramFiles(x86)") {
        kit_roots.push(
            PathBuf::from(pf86)
                .join("Windows Kits")
                .join("10")
                .join("bin"),
        );
    }
    if let Some(pf) = env::var_os("ProgramFiles") {
        kit_roots.push(
            PathBuf::from(pf)
                .join("Windows Kits")
                .join("10")
                .join("bin"),
        );
    }

    for bin_root in kit_roots {
        if let Some(found) = scan_windows_kit_bin(&bin_root, arch) {
            return Some(found);
        }
    }
    None
}

fn scan_windows_kit_bin(bin_root: &Path, arch: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(bin_root).ok()?;
    let mut versioned: Vec<(Vec<u32>, PathBuf)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let candidate = path.join(arch).join("rc.exe");
        if !candidate.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let parts: Option<Vec<u32>> = name_str.split('.').map(|p| p.parse::<u32>().ok()).collect();
        if let Some(ver) = parts {
            versioned.push((ver, candidate));
        }
    }

    versioned.sort_by(|a, b| b.0.cmp(&a.0));
    if let Some((_, best)) = versioned.into_iter().next() {
        return Some(best);
    }

    let unversioned = bin_root.join(arch).join("rc.exe");
    unversioned.is_file().then_some(unversioned)
}
