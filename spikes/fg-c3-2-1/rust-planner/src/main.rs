use std::io::{self, Read};

fn main() {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        println!("{{\"protocolVersion\":1,\"ok\":false,\"phase\":\"protocol\",\"diagnostics\":[{{\"code\":\"REQUEST_READ_FAILED\",\"path\":\"/\"}}]}}");
        std::process::exit(2);
    }
    println!("{}", fg_c3_2_1_rust_planner_spike::handle_json(&input));
}
