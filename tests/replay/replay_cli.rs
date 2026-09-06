use std::path::PathBuf;
use std::process::Command;

#[test]
fn state_at_prints_the_snapshot_after_the_requested_event() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tenhou/ranked_game.json");
    let output = Command::new(env!("CARGO_BIN_EXE_replay"))
        .args(["--event", "1", "--state-at", "1"])
        .arg(fixture)
        .output()
        .expect("replay CLI must run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("replay output must be UTF-8");
    let snapshot = stdout
        .split("========== STATE AFTER G001 ==========")
        .nth(1)
        .expect("state-at snapshot heading must be present");
    assert!(snapshot.contains("round=E1"));
    assert!(snapshot.contains("phase=Initial"));
    assert!(snapshot.contains("P0 score=25000"));
    assert!(!snapshot.contains("phase=Ended"));
}
