use fg_c3_2_1_rust_planner_spike::{
    bounded_response, error, handle_json, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES,
};
use std::io::{self, Read, Write};

#[cfg(feature = "fg-c3-7-test-oom")]
fn test_process_evidence(stage: &str, rss_kib: Option<u64>) -> io::Result<()> {
    let Ok(path) = std::env::var("FG_C3_7_TEST_EVIDENCE_FILE") else {
        return Ok(());
    };
    let cgroup_data = std::fs::read_to_string("/proc/self/cgroup")?;
    let cgroup = cgroup_data
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .unwrap_or("unavailable")
        .to_owned();
    let rss = rss_kib.map_or_else(|| "unavailable".to_owned(), |value| value.to_string());
    let content = format!(
        "stage={stage}\npid={}\ncgroup={cgroup}\nrss_kib={rss}\n",
        std::process::id()
    );
    std::fs::write(path, content)
}

#[cfg(feature = "fg-c3-7-test-oom")]
fn test_oom_if_armed() -> io::Result<()> {
    let Ok(marker) = std::env::var("FG_C3_7_TEST_OOM_TRIGGER_FILE") else {
        return Ok(());
    };
    if !std::path::Path::new(&marker).exists() {
        return Ok(());
    }
    let bytes = std::env::var("FG_C3_7_TEST_OOM_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(256 * 1024 * 1024);
    if !(64 * 1024 * 1024..=512 * 1024 * 1024).contains(&bytes) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "test-only OOM allocation must be between 64 and 512 MiB",
        ));
    }
    let _ = std::fs::remove_file(marker);
    let mut allocation = Vec::<u8>::new();
    test_process_evidence("oom-pressure-started", None)?;
    let page_size = 4096;
    let checkpoint = 1024 * 1024;
    while allocation.len() < bytes {
        let start = allocation.len();
        let end = (start + checkpoint).min(bytes);
        allocation.resize(end, 0);
        for page in (start..end).step_by(page_size) {
            allocation[page] = 1;
        }
        let status = std::fs::read_to_string("/proc/self/status")?;
        let rss_kib = status.lines().find_map(|line| {
            let rest = line.strip_prefix("VmRSS:")?;
            rest.split_whitespace().next()?.parse::<u64>().ok()
        });
        test_process_evidence("oom-pressure-progress", rss_kib)?;
    }
    std::hint::black_box(&allocation);
    std::thread::sleep(std::time::Duration::from_secs(30));
    Ok(())
}

#[cfg(not(feature = "fg-c3-7-test-oom"))]
fn test_oom_if_armed() -> io::Result<()> {
    Ok(())
}

#[cfg(feature = "fg-c3-7-test-oom")]
fn test_process_ready_evidence() -> io::Result<()> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    let rss_kib = status.lines().find_map(|line| {
        let rest = line.strip_prefix("VmRSS:")?;
        rest.split_whitespace().next()?.parse::<u64>().ok()
    });
    test_process_evidence("ready", rss_kib)
}

#[cfg(not(feature = "fg-c3-7-test-oom"))]
fn test_process_ready_evidence() -> io::Result<()> {
    Ok(())
}

fn write_frame(output: &mut impl Write, response: &str) -> io::Result<()> {
    let length = u32::try_from(response.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "response length overflow"))?;
    output.write_all(&length.to_be_bytes())?;
    output.write_all(response.as_bytes())?;
    output.flush()
}

fn main() -> io::Result<()> {
    test_process_ready_evidence()?;
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
        test_oom_if_armed()?;
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
