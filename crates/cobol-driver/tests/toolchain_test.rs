use std::path::Path;

#[test]
fn toolchain_resolves_direct_runtime_archive_from_driver_boundary() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let archive = dir.path().join("libcobol_runtime.a");
    std::fs::write(&archive, b"archive").expect("write archive");

    let resolved = cobol_driver::toolchain::resolve_runtime_archive_path(dir.path())
        .expect("runtime archive should resolve");

    assert_eq!(resolved, archive.canonicalize().unwrap());
}

#[test]
fn toolchain_reports_missing_runtime_archive_from_driver_boundary() {
    let err = cobol_driver::toolchain::resolve_runtime_archive_path(Path::new("missing-runtime"))
        .expect_err("missing archive should be reported");

    assert!(err.contains("COBOL runtime static library path"));
}

#[test]
fn toolchain_uses_newest_hashed_runtime_archive_from_driver_boundary() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let deps = dir.path().join("deps");
    std::fs::create_dir(&deps).expect("create deps dir");
    let old_archive = deps.join("libcobol_runtime-old.a");
    let new_archive = deps.join("libcobol_runtime-new.a");
    std::fs::write(&old_archive, b"old").expect("write old archive");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&new_archive, b"new").expect("write new archive");

    let resolved = cobol_driver::toolchain::resolve_runtime_archive_path(dir.path())
        .expect("runtime archive should resolve");

    assert_eq!(resolved, new_archive.canonicalize().unwrap());
}
