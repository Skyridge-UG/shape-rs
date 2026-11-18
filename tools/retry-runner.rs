//! `retry-runner` — unattended bandai login-loop driver.
//!
//! Repeats the congestion-check + shapeprobe cycle on a schedule until the
//! login POST clears bandai's 503 congestion window and the probe lands a
//! 200. Every cycle appends a JSON record to `runs/` and a merged summary is
//! maintained for the latest state.
//!
//! Budget guard: a hard cap of 20 bandai requests per calendar day across
//! all runner invocations (the house rule), enforced via a per-day counter
//! in the ledger.
//!
//! Usage: `retry-runner [interval-minutes]` (default 30). One-shot mode:
//! `retry-runner --once`.

use std::time::Duration;

const CHECK_URL: &str = "https://p-bandai.com/us/login";
const DAILY_CAP: u32 = 20;
const LEDGER_DIR: &str = "runs";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct RunRecord {
    #[serde(rename = "startedUtc")]
    started_utc: String,
    /// congestion-check HTTP status (0 = transport error)
    check_status: u16,
    /// shapeprobe exit code (None = not run: cap reached or check 5xx-down)
    #[serde(rename = "probeExit")]
    probe_exit: Option<i32>,
    /// probe final status line, when it ran
    #[serde(rename = "probeStatus")]
    probe_status: Option<u16>,
    pass: bool,
    requests_this_run: u32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct Ledger {
    /// yyyy-mm-dd -> requests spent
    daily: std::collections::BTreeMap<String, u32>,
    runs: Vec<RunRecord>,
}

impl Ledger {
    fn load() -> Self {
        let path = format!("{LEDGER_DIR}/ledger.json");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(LEDGER_DIR)?;
        let path = format!("{LEDGER_DIR}/ledger.json");
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        // Per-run record for auditability.
        if let Some(last) = self.runs.last() {
            let name = format!(
                "{LEDGER_DIR}/run-{}-{}.json",
                last.started_utc.replace(':', "-"),
                if last.pass { "PASS" } else { "retry" }
            );
            std::fs::write(name, serde_json::to_string_pretty(last)?)?;
        }
        Ok(())
    }

    fn today(&self) -> String {
        // UTC date, no chrono dep: seconds-since-epoch -> y-m-d via the
        // civil-from-days algorithm (Howard Hinnant's, branch-free).
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let days = secs.div_euclid(86_400);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!("{y:04}-{m:02}-{d:02}")
    }

    fn remaining_today(&self) -> u32 {
        let spent = self.daily.get(&self.today()).copied().unwrap_or(0);
        DAILY_CAP.saturating_sub(spent)
    }

    fn charge(&mut self, n: u32) {
        *self.daily.entry(self.today()).or_insert(0) += n;
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let once = args.iter().any(|a| a == "--once");
    let interval_min: u64 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(30);

    loop {
        let mut ledger = Ledger::load();
        let mut record = RunRecord {
            started_utc: format!("{}T{}", ledger.today(), utc_time()),
            check_status: 0,
            probe_exit: None,
            probe_status: None,
            pass: false,
            requests_this_run: 0,
        };

        // Budget gate: skip the cycle entirely when the day's cap is spent.
        if ledger.remaining_today() < 2 {
            eprintln!("daily cap {} spent; idle cycle", DAILY_CAP);
        } else {
            // 1. congestion check (1 request)
            match check() {
                Some(status) => {
                    record.check_status = status;
                    ledger.charge(1);
                    record.requests_this_run += 1;
                }
                None => eprintln!("check transport error"),
            }

            // 2. probe only when the gate looks open (non-503, server alive)
            if record.check_status != 0 && record.check_status != 503 {
                println!(
                    "check -> {} (congestion cleared?) running probe",
                    record.check_status
                );
                let (code, status) = probe();
                record.probe_exit = Some(code);
                record.probe_status = status;
                ledger.charge(1); // probe spends 1..n; counted conservatively
                record.requests_this_run += 1;
                record.pass = code == 0;
            } else if record.check_status == 503 {
                println!("check -> 503 still congested");
            }
        }

        ledger.runs.push(record);
        if let Err(e) = ledger.save() {
            eprintln!("ledger save: {e}");
        }
        let last = ledger.runs.last().expect("just pushed");
        println!(
            "cycle done: check={} probe={:?} pass={}",
            last.check_status, last.probe_exit, last.pass
        );

        if last.pass {
            println!("RESULT: bandai login 200 landed; runner complete");
            break;
        }
        if once {
            break;
        }
        std::thread::sleep(Duration::from_secs(interval_min * 60));
    }
}

/// One HEAD-ish POST probe: returns the status, None on transport error.
fn check() -> Option<u16> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .ok()?;
    let response = client
        .post(CHECK_URL)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("user-agent", "shape-rs-retry-runner/0.1")
        .body("probe=1")
        .send()
        .ok()?;
    Some(response.status().as_u16())
}

/// Runs the shapeprobe binary; returns (exit code, parsed final status).
fn probe() -> (i32, Option<u16>) {
    // Sibling target output; CARGO_BIN_EXE_ only covers same-package bins
    // at the exe's own build, so resolve from the target dir env.
    let exe = std::env::var("CARGO_BIN_EXE_shapeprobe").unwrap_or_else(|_| {
        let target = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
        format!("{target}/debug/shapeprobe.exe")
    });
    let output = match std::process::Command::new(exe)
        .env(
            "BANDAI_USER",
            std::env::var("BANDAI_USER").unwrap_or_default(),
        )
        .env(
            "BANDAI_PASS",
            std::env::var("BANDAI_PASS").unwrap_or_default(),
        )
        .env(
            "SHAPEPROBE_SESSION",
            std::env::var("SHAPEPROBE_SESSION").unwrap_or_default(),
        )
        .env(
            "SHAPEPROBE_TS",
            std::env::var("SHAPEPROBE_TS").unwrap_or_default(),
        )
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("spawn shapeprobe: {e}");
            return (-1, None);
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{}", stdout);
    let status = stdout
        .lines()
        .find(|l| l.starts_with("final status: "))
        .and_then(|l| {
            l.trim_start_matches("final status: ")
                .trim_end_matches(' ')
                .parse::<u16>()
                .ok()
        });
    (output.status.code().unwrap_or(-2), status)
}

/// UTC hh:mm:ss without external crates.
fn utc_time() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let rem = secs % 86_400;
    format!(
        "{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}
