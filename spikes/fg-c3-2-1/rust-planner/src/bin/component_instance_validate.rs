//! Non-production JSONL test runner for the isolated Component/Instance
//! validator. It is not linked into the Go planner protocol.

use fg_c3_2_1_rust_planner_spike::component_instance_validator::validate_component_instance_json;
use std::io::{self, BufRead, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let output = validate_component_instance_json(&line);
        serde_json::to_writer(&mut stdout, &output)?;
        stdout.write_all(b"\n")?;
    }
    stdout.flush()?;
    Ok(())
}
