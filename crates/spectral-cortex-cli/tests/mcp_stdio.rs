use std::io::Write;
use std::process::{Command, Stdio};

use tempfile::NamedTempFile;

fn empty_smg_fixture() -> NamedTempFile {
    let fixture = NamedTempFile::new().expect("create SMG fixture");
    std::fs::write(
        fixture.path(),
        r#"{"metadata":{"format_version":"spectral-cortex-v1"},"notes":[],"cluster_labels":null,"cluster_centroids":null,"cluster_centroid_norms":null,"long_range_links":null}"#,
    )
    .expect("write SMG fixture");
    fixture
}

#[test]
fn malformed_json_rpc_does_not_panic_or_write_stdout() {
    let fixture = empty_smg_fixture();
    let mut child = Command::new(env!("CARGO_BIN_EXE_spectral-cortex"))
        .args(["mcp", "--smg"])
        .arg(fixture.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start MCP server");

    child
        .stdin
        .take()
        .expect("MCP stdin")
        .write_all(b"not valid json-rpc\n")
        .expect("write malformed JSON-RPC");
    let output = child.wait_with_output().expect("wait for MCP server");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked at"));
}

#[test]
fn eof_shuts_down_cleanly_without_stdout_diagnostics() {
    let fixture = empty_smg_fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_spectral-cortex"))
        .args(["mcp", "--smg"])
        .arg(fixture.path())
        .output()
        .expect("run MCP server");

    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked at"));
}
