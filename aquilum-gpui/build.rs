use std::path::{Path, PathBuf};

/// Locates the directory holding `rc.exe` (the resource compiler) among the installed
/// Windows SDKs, without touching the registry.
///
/// Why this is here. By default `winres` resolves the SDK path by shelling out to
/// `reg query "HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots"`. Where `reg.exe`
/// cannot run (blocked by policy, trimmed PATH, CI sandbox), the lookup fails silently:
/// `rc.exe` is never found and the resources — above all the **icon** — never reach the
/// exe. The build still succeeds, merely with a `warning`, so the missing icon is easy
/// to overlook.
///
/// So we resolve the `rc.exe` directory ourselves and pass it through
/// `WindowsResource::set_toolkit_path`. The returned path is the directory that actually
/// contains `rc.exe` (`...\Windows Kits\10\bin\<version>\<arch>`), because winres checks
/// `<toolkit_path>\rc.exe` first.
fn find_rc_bin_dir() -> Option<PathBuf> {
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => return None,
    };

    let roots = [
        Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin"),
        Path::new(r"C:\Program Files\Windows Kits\10\bin"),
    ];

    let mut candidates: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            // Skip the shorthand `x64` / `x86` / `arm` directories — a version starts
            // with a digit (`10.0.19041.0`).
            let is_version = dir
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.chars().next())
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false);
            if !is_version {
                continue;
            }
            let bin = dir.join(arch);
            if bin.join("rc.exe").is_file() {
                candidates.push(bin);
            }
        }
    }

    // Highest SDK version wins — the same rule winres itself applies (`v.pop()` after sorting).
    candidates.sort_by_key(|p| version_key(p));
    candidates.pop()
}

/// SDK version sort key: `10.0.19041.0` -> `[10, 0, 19041, 0]`.
fn version_key(bin_dir: &Path) -> Vec<u64> {
    bin_dir
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut res = winres::WindowsResource::new();

    match find_rc_bin_dir() {
        Some(bin_dir) => {
            res.set_toolkit_path(&bin_dir.to_string_lossy());
        }
        None => println!(
            "cargo:warning=rc.exe не найден ни в одном установленном Windows SDK — \
             иконка НЕ будет встроена в exe"
        ),
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let icon_path = Path::new(&manifest_dir).join("assets").join("iconi.ico");
    if icon_path.exists() {
        res.set_icon(icon_path.to_str().unwrap());
    } else {
        println!("cargo:warning=assets/iconi.ico not found");
    }

    if let Err(e) = res.compile() {
        println!("cargo:warning=winres compile failed: {e} — exe будет собран без иконки");
    }

    // Stack reserve for GPUI on Windows (the default 1 MB may be too small for deep nesting).
    println!("cargo:rustc-link-arg=/STACK:33554432");
}
