use fg_c3_2_1_rust_planner_spike::{
    bounded_response, error, handle_json, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES,
};
use std::io::{self, Read, Write};

fn write_frame(output: &mut impl Write, response: &str) -> io::Result<()> {
    let length = u32::try_from(response.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "response length overflow"))?;
    output.write_all(&length.to_be_bytes())?;
    output.write_all(response.as_bytes())?;
    output.flush()
}

fn main() -> io::Result<()> {
    let response_limit = std::env::var("FG_C3_2_2_TEST_RESPONSE_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(MAX_RESPONSE_BYTES)
        .min(MAX_RESPONSE_BYTES);
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();

    loop {
        let mut header = [0_u8; 4];
        let first = match input.read(&mut header[..1]) {
            Ok(0) => return Ok(()),
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if first != 1 {
            return Ok(());
        }
        if input.read_exact(&mut header[1..]).is_err() {
            write_frame(
                &mut output,
                &error(
                    None,
                    "FRAME_TRUNCATED",
                    "/frame/header",
                    "Frame header ended before four bytes.",
                ),
            )?;
            return Ok(());
        }
        let request_len = u32::from_be_bytes(header) as usize;
        if request_len > MAX_REQUEST_BYTES {
            let response = error(
                None,
                "REQUEST_TOO_LARGE",
                "/frame",
                "Request frame exceeds the protocol limit.",
            );
            write_frame(&mut output, &response)?;
            return Ok(());
        }
        let mut request = vec![0_u8; request_len];
        if input.read_exact(&mut request).is_err() {
            write_frame(
                &mut output,
                &error(
                    None,
                    "FRAME_TRUNCATED",
                    "/frame/body",
                    "Frame body ended before its declared length.",
                ),
            )?;
            return Ok(());
        }
        let raw = match std::str::from_utf8(&request) {
            Ok(raw) => raw,
            Err(_) => {
                let response = error(
                    None,
                    "REQUEST_INVALID_UTF8",
                    "/frame/body",
                    "Request frame is not UTF-8.",
                );
                write_frame(&mut output, &response)?;
                continue;
            }
        };
        let response = std::panic::catch_unwind(|| handle_json(raw)).unwrap_or_else(|_| {
            error(
                None,
                "PLANNER_INTERNAL_ERROR",
                "/",
                "Planner failed while evaluating the request.",
            )
        });
        let request_id = serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|value| {
                value
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .filter(|id| !id.is_empty() && id.len() <= 128)
                    .map(str::to_owned)
            });
        let response = bounded_response(response, response_limit, request_id.as_deref());
        write_frame(&mut output, &response)?;
    }
}
