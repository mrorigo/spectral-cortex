use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn torch_lib_dir(build_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(build_dir).ok()?;
    let mut candidates: Vec<(SystemTime, PathBuf)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !name.starts_with("torch-sys-") {
            continue;
        }

        let candidate = path.join("out/libtorch/libtorch/lib");
        let marker = candidate.join("libtorch_cpu.dylib");
        if marker.exists() {
            let modified = fs::metadata(&marker)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            candidates.push((modified, candidate));
        }
    }

    candidates
        .into_iter()
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let out_dir = match env::var_os("OUT_DIR") {
        Some(v) => PathBuf::from(v),
        None => return,
    };

    // OUT_DIR: target/<profile>/build/<crate-hash>/out
    let build_dir = match out_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
    {
        Some(p) => p,
        None => return,
    };

    let profile_dir = match build_dir.parent().map(Path::to_path_buf) {
        Some(p) => p,
        None => return,
    };

    if let Some(lib_dir) = torch_lib_dir(&build_dir) {
        let stable_runtime_dir = profile_dir.join("libtorch");
        let _ = fs::create_dir_all(&stable_runtime_dir);

        if let Ok(entries) = fs::read_dir(&lib_dir) {
            for entry in entries.flatten() {
                let src = entry.path();
                let is_dylib = src
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s == "dylib")
                    .unwrap_or(false);
                if !is_dylib {
                    continue;
                }

                if let Some(name) = src.file_name() {
                    let dst = stable_runtime_dir.join(name);
                    let _ = fs::copy(&src, &dst);
                }
            }
        }

        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/libtorch");
    }
}
