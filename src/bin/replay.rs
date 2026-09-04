use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read};

use convlog::tenhou::Log;
use convlog::tenhou_to_mjai;
use kyoku::replay::inspector::ReplayInspector;

const USAGE: &str = "Usage: cargo run --bin replay -- [--full-state] <tenhou-json|->\n\nRead a tenhou.net/6 JSON log from a file or stdin (-), then print replay deltas.\n\nOptions:\n  -f, --full-state  print the final full RoundState snapshot\n  -h, --help        show this help\n";

fn main() {
    if let Err(error) = run() {
        eprintln!("replay: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut full_state = false;
    let mut input_path = None;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "-f" | "--full-state" => full_state = true,
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option {value}\n\n{USAGE}").into());
            }
            value if input_path.is_none() => input_path = Some(value.to_owned()),
            _ => return Err(format!("expected one input path\n\n{USAGE}").into()),
        }
    }

    let input_path = input_path.ok_or_else(|| format!("missing input path\n\n{USAGE}"))?;
    let json = read_input(&input_path)?;
    let log = Log::from_json_str(&json)?;
    let events = tenhou_to_mjai(&log)?;
    let mut inspector = ReplayInspector::new();
    for event in &events {
        inspector.apply(event).map_err(|error| *error)?;
    }

    print!("{}", inspector.output());
    if full_state {
        println!("\n========== FINAL STATE ==========");
        print!("{}", inspector.full_state());
    }
    Ok(())
}

fn read_input(path: &str) -> Result<String, io::Error> {
    if path == "-" {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        Ok(input)
    } else {
        fs::read_to_string(path)
    }
}
