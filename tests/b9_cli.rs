use std::process::Command;

#[test]
fn version_prints_exact_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_b9"))
        .arg("--version")
        .output()
        .expect("run b9 --version");

    assert!(output.status.success());
    assert_eq!(output.stdout, b"b9 0.1.0\n");
    assert!(output.stderr.is_empty());
}
