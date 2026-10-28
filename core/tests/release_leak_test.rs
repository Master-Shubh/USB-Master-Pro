//! Release Artifact Cryptographic Leak Regression Test
//! Validates that release binaries and libraries contain zero embedded root private seeds,
//! private keys, or internal CA fixtures.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn test_verify_no_leaks_script_passes() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().expect("Failed to locate repo root");
    let script_path = repo_root.join("scripts").join("verify_no_leaks.sh");

    assert!(script_path.exists(), "verify_no_leaks.sh must exist at {:?}", script_path);

    let output = Command::new("bash")
        .arg(&script_path)
        .current_dir(repo_root)
        .output()
        .expect("Failed to execute verify_no_leaks.sh");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("STDOUT:\n{}", stdout);
    if !stderr.is_empty() {
        eprintln!("STDERR:\n{}", stderr);
    }

    assert!(
        output.status.success(),
        "verify_no_leaks.sh failed with exit code {:?}. Output: {}",
        output.status.code(),
        stdout
    );
    assert!(
        stdout.contains("LEAK CHECK PASSED"),
        "Script output must confirm LEAK CHECK PASSED"
    );
}
