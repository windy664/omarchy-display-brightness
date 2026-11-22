//! Display brightness for Omarchy that keeps working when DDC does not.
//!
//! An external monitor that does not answer DDC/CI cannot be dimmed through
//! hardware. Omarchy's own `omarchy-brightness-display` exits non-zero in
//! that case, and the brightness keys stop doing anything. This tool tries
//! the hardware path first and falls back to `wl-gammarelay-rs`, which dims
//! the picture in software.
//!
//! Usage: `display-brightness +3% | 3%- | 100% | 1%`

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "\
display-brightness -- display brightness that survives a monitor without DDC

Usage:
  display-brightness <step>

  +N%     brighter by N percent
  N%-     dimmer by N percent
  N%      set the brightness to N percent

  Examples:
    display-brightness +3%
    display-brightness 3%-
    display-brightness 100%
";

/// How long a failed DDC attempt is remembered, in seconds.
///
/// Probing DDC costs a few hundred milliseconds and the keys repeat, so the
/// failure is cached: the next presses go straight to the software path.
const DDC_FAILURE_TTL_SECONDS: u64 = 60;

const GAMMARELAY_BUS: &str = "rs.wl-gammarelay";
const GAMMARELAY_PATH: &str = "/";
const GAMMARELAY_INTERFACE: &str = "rs.wl.gammarelay";
const GAMMARELAY_PROPERTY: &str = "Brightness";

/// What the user asked for.
enum Step {
    /// A relative change, in the range -1.0 ..= 1.0.
    Delta(f64),
    /// An absolute level, in the range 0.0 ..= 1.0.
    Absolute(f64),
}

/// Parse `+3%`, `3%-` and `12%`.
fn parse_step(text: &str) -> Option<Step> {
    let text = text.trim();

    if let Some(rest) = text.strip_prefix('+') {
        let percent = rest.strip_suffix('%')?.parse::<f64>().ok()?;
        return Some(Step::Delta(percent / 100.0));
    }

    if let Some(rest) = text.strip_suffix('-') {
        let percent = rest.strip_suffix('%')?.parse::<f64>().ok()?;
        return Some(Step::Delta(-percent / 100.0));
    }

    let percent = text.strip_suffix('%')?.parse::<f64>().ok()?;
    Some(Step::Absolute((percent / 100.0).clamp(0.0, 1.0)))
}

/// Where the DDC failure is remembered between presses.
fn cache_file() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    runtime.join("display-brightness.no-ddc")
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Whether the last DDC attempt failed recently enough to skip another.
fn ddc_recently_failed() -> bool {
    let Ok(stamp) = std::fs::read_to_string(cache_file()) else {
        return false;
    };
    let Ok(stamp) = stamp.trim().parse::<u64>() else {
        return false;
    };

    now_seconds().saturating_sub(stamp) < DDC_FAILURE_TTL_SECONDS
}

fn remember_ddc_failure() {
    let _ = std::fs::write(cache_file(), now_seconds().to_string());
}

/// Try the hardware path. Returns whether it worked.
fn try_ddc(step: &str) -> bool {
    if ddc_recently_failed() {
        return false;
    }

    let worked = Command::new("omarchy-brightness-display")
        .arg(step)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    if !worked {
        remember_ddc_failure();
    }

    worked
}

/// Make sure the software dimmer is running.
fn ensure_gammarelay() {
    let running = Command::new("pgrep")
        .args(["-x", "wl-gammarelay-rs"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    if running {
        return;
    }

    // setsid detaches it from this process group, so it survives the exit.
    let _ = Command::new("setsid")
        .arg("wl-gammarelay-rs")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Read the current software brightness.
fn gammarelay_brightness() -> Option<f64> {
    let output = Command::new("busctl")
        .args(["--user", "get-property"])
        .args([GAMMARELAY_BUS, GAMMARELAY_PATH, GAMMARELAY_INTERFACE])
        .arg(GAMMARELAY_PROPERTY)
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    // busctl prints "d 1" for a double property.
    let value = text.split_whitespace().nth(1)?;

    value.parse::<f64>().ok()
}

/// Set the software brightness outright.
fn set_gammarelay(brightness: f64) -> Result<(), String> {
    let status = Command::new("busctl")
        .args(["--user", "set-property"])
        .args([GAMMARELAY_BUS, GAMMARELAY_PATH, GAMMARELAY_INTERFACE])
        .arg(GAMMARELAY_PROPERTY)
        .arg("d")
        .arg(brightness.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("could not run busctl: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("busctl could not set the brightness".to_string())
    }
}

/// Move the software brightness by a delta.
fn shift_gammarelay(delta: f64) -> Result<(), String> {
    let status = Command::new("busctl")
        .args(["--user", "call"])
        .args([GAMMARELAY_BUS, GAMMARELAY_PATH, GAMMARELAY_INTERFACE])
        .arg("UpdateBrightness")
        .arg("d")
        .arg("--")
        .arg(delta.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("could not run busctl: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("busctl could not update the brightness".to_string())
    }
}

/// Show the on-screen display, the way the hardware path does.
fn show_osd(brightness: f64) {
    let percent = (brightness * 100.0).round().clamp(0.0, 100.0) as i64;

    let _ = Command::new("omarchy-osd")
        .args(["-i", "brightness", "-p", &percent.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// The software path.
fn apply_software(step: Step) -> Result<(), String> {
    ensure_gammarelay();

    match step {
        Step::Absolute(brightness) => {
            set_gammarelay(brightness)?;
            show_osd(brightness);
        }
        Step::Delta(delta) => {
            shift_gammarelay(delta)?;
            show_osd(gammarelay_brightness().unwrap_or(0.0));
        }
    }

    Ok(())
}

fn main() {
    let Some(argument) = std::env::args().nth(1) else {
        eprint!("{USAGE}");
        std::process::exit(1);
    };

    if argument == "-h" || argument == "--help" || argument == "help" {
        print!("{USAGE}");
        return;
    }

    let Some(step) = parse_step(&argument) else {
        eprintln!("display-brightness: cannot read '{argument}'");
        eprint!("{USAGE}");
        std::process::exit(1);
    };

    // The hardware path also draws the OSD, so it is all that is needed when
    // the monitor answers DDC.
    if try_ddc(&argument) {
        return;
    }

    if let Err(error) = apply_software(step) {
        eprintln!("display-brightness: {error}");
        std::process::exit(1);
    }
}
