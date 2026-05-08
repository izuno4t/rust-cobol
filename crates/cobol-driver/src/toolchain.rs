use std::path::{Path, PathBuf};
use std::process::Command;

pub fn resolve_runtime_archive_path(runtime_lib_path: &Path) -> Result<PathBuf, String> {
    fn find_archive(dir: &Path) -> Option<PathBuf> {
        let direct = dir.join("libcobol_runtime.a");
        if direct.exists() {
            return direct.canonicalize().ok().or(Some(direct));
        }

        let mut newest: Option<PathBuf> = None;
        for search_dir in [dir.to_path_buf(), dir.join("deps")] {
            let entries = match std::fs::read_dir(&search_dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if name.starts_with("libcobol_runtime") && name.ends_with(".a") {
                    let is_newer = newest
                        .as_ref()
                        .map(|current| archive_is_newer(&path, current))
                        .unwrap_or(true);
                    if is_newer {
                        newest = Some(path);
                    }
                }
            }
        }
        newest.map(|path| path.canonicalize().ok().unwrap_or(path))
    }

    let mut candidates = vec![runtime_lib_path.to_path_buf()];
    if runtime_lib_path.is_relative() {
        if let Ok(cwd) = std::env::current_dir() {
            for ancestor in cwd.ancestors() {
                candidates.push(ancestor.join(runtime_lib_path));
            }
        }
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let manifest_dir = PathBuf::from(manifest_dir);
            for ancestor in manifest_dir.ancestors() {
                candidates.push(ancestor.join(runtime_lib_path));
            }
        }
    }

    let candidate = candidates
        .into_iter()
        .find_map(|candidate| {
            if candidate.is_file() {
                Some(candidate)
            } else if candidate.is_dir() {
                find_archive(&candidate)
            } else {
                None
            }
        })
        .ok_or_else(|| {
            format!(
                "COBOL runtime static library path '{}' does not exist",
                runtime_lib_path.display()
            )
        })?;

    candidate.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve runtime library '{}': {}",
            candidate.display(),
            e
        )
    })
}

fn archive_is_newer(candidate: &Path, current: &Path) -> bool {
    let candidate_modified = candidate
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok();
    let current_modified = current
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok();
    match (candidate_modified, current_modified) {
        (Some(candidate_modified), Some(current_modified)) => candidate_modified > current_modified,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => candidate.file_name() > current.file_name(),
    }
}

pub fn compile_c_to_executable(
    c_source_path: &Path,
    output_path: &Path,
    runtime_lib_path: &Path,
) -> Result<(), String> {
    let compiler = find_c_compiler()?;
    let runtime_archive = resolve_runtime_archive_path(runtime_lib_path)?;

    let mut cmd = Command::new(&compiler);
    cmd.arg("-O2")
        .arg("-fno-strict-aliasing")
        .arg(c_source_path)
        .arg("-o")
        .arg(output_path)
        .arg(runtime_archive);

    if cfg!(windows) {
        cmd.arg("-lws2_32");
    } else {
        cmd.arg("-lpthread").arg("-ldl");
    }

    let status = cmd
        .arg("-lm")
        .status()
        .map_err(|e| format!("Failed to run C compiler '{}': {}", compiler, e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "C compiler '{}' exited with status: {}",
            compiler, status
        ))
    }
}

fn find_c_compiler() -> Result<String, String> {
    if let Ok(cc) = std::env::var("CC") {
        return Ok(cc);
    }

    for compiler in &["clang", "gcc", "cc"] {
        if Command::new(compiler).arg("--version").output().is_ok() {
            return Ok(compiler.to_string());
        }
    }

    Err("No C compiler found. Install clang or gcc.".to_string())
}
