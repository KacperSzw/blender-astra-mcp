use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn addon_zip_installs_in_isolated_blender() {
    let Some(exe) = std::env::var_os("BLENDER_EXE") else {
        eprintln!("Set BLENDER_EXE for ZIP installation test");
        return;
    };
    let scratch = tempfile::tempdir().unwrap();
    let archive = scratch.path().join("compact_blender.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in std::fs::read_dir(root.join("addon/compact_blender")).unwrap() {
        let file = file.unwrap().path();
        if file.extension().is_some_and(|e| e == "py") {
            zip.start_file(
                format!(
                    "compact_blender/{}",
                    file.file_name().unwrap().to_str().unwrap()
                ),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(&std::fs::read(file).unwrap()).unwrap();
        }
    }
    zip.finish().unwrap();
    std::fs::create_dir_all(scratch.path().join("scripts/addons")).unwrap();
    let log = std::fs::File::create(scratch.path().join("install.log")).unwrap();
    let mut child = Command::new(exe)
        .args([
            "--background",
            "--factory-startup",
            "--threads",
            "1",
            "--python-exit-code",
            "1",
            "--python",
        ])
        .arg(root.join("scripts/package_smoke.py"))
        .arg("--")
        .arg(archive)
        .arg(scratch.path())
        .env("BLENDER_USER_SCRIPTS", scratch.path().join("scripts"))
        .env("BLENDER_USER_CONFIG", scratch.path().join("config"))
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("ZIP installation timed out");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let log = std::fs::read_to_string(scratch.path().join("install.log")).unwrap();
    assert!(status.success(), "{log}");
    assert!(log.contains("PACKAGE_SMOKE_OK"), "{log}");
}
