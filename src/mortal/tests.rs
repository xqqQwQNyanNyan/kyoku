use super::*;

fn command(body: &str) -> Command {
    let ready = format!(
        r#"{{"version":4,"tag":"test-only","sha256":"{}"}}"#,
        "0".repeat(64)
    );
    let mut command = Command::new("sh");
    command.args(["-c", &format!("printf '%s\\n' '{ready}'\n{body}")]);
    command
}

#[test]
fn process_protocol_handles_multiple_events_and_clean_shutdown() {
    let mut mortal = Mortal::spawn(
        command(
            r#"
        while IFS= read -r event; do
            printf '%s\n' '{"type":"none","meta":{"mask_bits":0}}'
        done
    "#,
        ),
        PlayerIndex::new(0).unwrap(),
    )
    .unwrap();
    assert_eq!(mortal.model().tag, "test-only");
    assert!(mortal.react(&Event::EndKyoku).unwrap().is_none());
    assert!(mortal.react(&Event::EndGame).unwrap().is_none());
    mortal.finish().unwrap();
}

#[test]
fn bad_json_closes_session_instead_of_shifting_subsequent_responses() {
    let mut mortal = Mortal::spawn(
        command(
            r#"
        IFS= read -r event
        printf '%s\n' 'broken'
    "#,
        ),
        PlayerIndex::new(0).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        mortal.react(&Event::EndGame),
        Err(MortalError::Json(_))
    ));
    assert!(matches!(
        mortal.react(&Event::EndGame),
        Err(MortalError::Closed)
    ));
}

#[test]
fn early_eof_and_nonzero_exit_are_reported() {
    let mut mortal = Mortal::spawn(
        command("IFS= read -r event\nexit 1"),
        PlayerIndex::new(0).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        mortal.react(&Event::EndGame),
        Err(MortalError::UnexpectedEof)
    ));
    let mortal = Mortal::spawn(
        command("while IFS= read -r event; do :; done\nexit 7"),
        PlayerIndex::new(0).unwrap(),
    )
    .unwrap();
    assert!(matches!(mortal.finish(), Err(MortalError::Exit(status)) if status.code() == Some(7)));
}

#[test]
fn startup_failure_is_reported() {
    let result = Mortal::spawn(
        Command::new("/nonexistent-kyoku-mortal-python"),
        PlayerIndex::new(0).unwrap(),
    );
    assert!(matches!(result, Err(MortalError::Spawn(_))));
}

#[test]
fn east_only_game_is_rejected_before_inference_and_closes_session() {
    let mut mortal = Mortal::spawn(
        command("while IFS= read -r event; do exit 9; done"),
        PlayerIndex::new(0).unwrap(),
    )
    .unwrap();
    let event = Event::StartGame {
        kyoku_first: 4,
        aka_flag: true,
        names: std::array::from_fn(|i| format!("P{i}")),
    };
    assert!(matches!(
        mortal.react(&event),
        Err(MortalError::UnsupportedGameLength)
    ));
    assert!(matches!(
        mortal.react(&Event::EndGame),
        Err(MortalError::Closed)
    ));
}
