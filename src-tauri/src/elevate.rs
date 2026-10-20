//! Running PowerShell from the app: hidden (no window) for reads, and
//! elevated through one UAC prompt for the few things Windows protects
//! (hosts file, Internet Connection Sharing).

use std::time::Duration;

/// Single-quoted PowerShell string literal.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// First meaningful line of PowerShell output (its errors add "At …" and
/// "+ ~~~" lines nobody needs to read).
pub fn first_line(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string()
}

/// Unique scratch name so overlapping calls never share (and delete) a file.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn hidden_script_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    format!("hidden-{}-{}.ps1", std::process::id(), N.fetch_add(1, Ordering::Relaxed))
}

/// Wrap an elevated script so any failure is written to `err_path` and the
/// process exits non-zero. Without this `Start-Process -Wait` hides errors.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn wrap_elevated_body(body: &str, err_path: &str) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'\r\n$step = ''\r\ntry {{\r\n{body}\r\n}} catch {{\r\n  $where = if ($step) {{ \"$step: \" }} else {{ '' }}\r\n  \"$where$($_.Exception.Message)\" | Out-File -LiteralPath {} -Encoding utf8\r\n  exit 1\r\n}}\r\n",
        ps_quote(err_path)
    )
}

/// Outer (non-elevated) PowerShell command: launch `script` elevated, wait,
/// and exit with the elevated process's exit code.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn launcher_command(script: &str) -> String {
    format!(
        "$p = Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','\"{script}\"'; exit $p.ExitCode"
    )
}

/// Run a command with piped output, killing it after `timeout`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn wait_with_timeout(cmd: &mut std::process::Command, timeout: Duration) -> Result<std::process::Output, String> {
    use std::io::Read;
    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not launch {:?}: {e}", cmd.get_program()))?;
    // Drain both pipes on threads so a chatty child never blocks on a full pipe.
    let mut out_pipe = child.stdout.take().expect("piped stdout");
    let mut err_pipe = child.stderr.take().expect("piped stderr");
    let out_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = out_pipe.read_to_end(&mut v);
        v
    });
    let err_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = err_pipe.read_to_end(&mut v);
        v
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("timed out after {} s", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let stdout = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    Ok(std::process::Output { status, stdout, stderr })
}

#[cfg(windows)]
fn scratch_dir() -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join("hotspot-pool");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[cfg(windows)]
fn powershell(args: &[&str], timeout: Duration) -> Result<std::process::Output, String> {
    // CREATE_NO_WINDOW: a console app would otherwise flash a window before
    // -WindowStyle Hidden takes effect.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden"]).args(args).creation_flags(CREATE_NO_WINDOW);
    wait_with_timeout(&mut cmd, timeout).map_err(|e| format!("PowerShell: {e}"))
}

/// Run `body` in a hidden, non-elevated PowerShell and return its stdout.
/// Gives up after 15 s so a stuck COM call cannot wedge the caller.
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn run_hidden(body: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        let script = scratch_dir()?.join(hidden_script_name());
        std::fs::write(&script, body).map_err(|e| e.to_string())?;
        let out = powershell(&["-ExecutionPolicy", "Bypass", "-File", &script.display().to_string()], Duration::from_secs(15));
        let _ = std::fs::remove_file(&script);
        let out = out?;
        if !out.status.success() {
            return Err(first_line(&String::from_utf8_lossy(&out.stderr)));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
    #[cfg(not(windows))]
    {
        Err("Not supported on this OS".into())
    }
}

/// Write `body` to `%TEMP%\hotspot-pool\<script_name>` and run it elevated
/// (one UAC prompt), waiting for it to finish. Errors raised inside the
/// elevated script come back as the Err message.
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn run_elevated(script_name: &str, body: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let dir = scratch_dir()?;
        let script = dir.join(script_name);
        let err_file = dir.join(format!("{script_name}.err"));
        let _ = std::fs::remove_file(&err_file);
        std::fs::write(&script, wrap_elevated_body(body, &err_file.display().to_string())).map_err(|e| e.to_string())?;
        // The user may take a while at the UAC prompt: allow 3 minutes.
        let out = powershell(&["-Command", &launcher_command(&script.display().to_string())], Duration::from_secs(180))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            if err.contains("canceled") || err.contains("cancelled") || err.contains("1223") {
                return Err("The Windows permission prompt was cancelled.".into());
            }
            if let Ok(msg) = std::fs::read_to_string(&err_file) {
                let _ = std::fs::remove_file(&err_file);
                return Err(first_line(&msg));
            }
            let line = first_line(&err);
            return Err(if line.is_empty() { format!("exit code {}", out.status.code().unwrap_or(-1)) } else { line });
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("Not supported on this OS".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_script_names_are_unique() {
        assert_ne!(hidden_script_name(), hidden_script_name());
    }

    #[test]
    fn elevated_wrapper_reports_failures() {
        let body = wrap_elevated_body("Do-Thing\r\n", "C:\\t\\apply.err");
        assert!(body.contains("try {"));
        assert!(body.contains("Do-Thing"));
        assert!(body.contains("catch {"));
        assert!(body.contains("'C:\\t\\apply.err'"));
        assert!(body.contains("exit 1"));
        assert!(body.contains("${step}: "));
        let launch = launcher_command("C:\\t\\apply.ps1");
        assert!(launch.contains("-PassThru"));
        assert!(launch.contains("exit $p.ExitCode"));
        assert!(launch.contains("C:\\t\\apply.ps1"));
    }

    #[test]
    fn first_line_trims_powershell_noise() {
        assert_eq!(first_line("Access denied\r\nAt C:\\x.ps1:2 char:6\r\n+ ~~~"), "Access denied");
        assert_eq!(first_line("\r\n  only  \r\n"), "only");
        assert_eq!(first_line(""), "");
    }

    #[test]
    fn wait_with_timeout_kills_slow_commands() {
        let mut slow = std::process::Command::new("sh");
        slow.args(["-c", "sleep 5"]);
        let err = wait_with_timeout(&mut slow, std::time::Duration::from_millis(300)).unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        let mut fast = std::process::Command::new("sh");
        fast.args(["-c", "echo hi"]);
        let out = wait_with_timeout(&mut fast, std::time::Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
    }

    /// Every PowerShell script the app generates must at least parse. Needs
    /// `pwsh` (PowerShell 7) on PATH or in HPF_PWSH; skipped otherwise.
    #[test]
    fn generated_powershell_parses() {
        let pwsh = std::env::var("HPF_PWSH").ok().unwrap_or_else(|| "pwsh".into());
        if std::process::Command::new(&pwsh).arg("-v").output().is_err() {
            eprintln!("pwsh not available; skipping parse check");
            return;
        }
        let dir = std::env::temp_dir().join(format!("hpf-ps-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let scripts: Vec<(&str, String)> = vec![
            ("apply-ics.ps1", wrap_elevated_body(&crate::ics::apply_script("abcdef01-1234-5678-9abc-def012345678", "99999999-8888-7777-6666-555555555555").unwrap(), "C:\\t\\apply-ics.ps1.err")),
            ("apply-hosts.ps1", wrap_elevated_body(&crate::names::hosts_script("C:\\t\\hosts.new", "C:\\Windows\\System32\\drivers\\etc\\hosts"), "C:\\t\\apply-hosts.ps1.err")),
            ("read-ics.ps1", crate::ics::read_script().to_string()),
            ("wfd-adapters.ps1", crate::wfd::go_adapter_script().to_string()),
            ("launcher.ps1", launcher_command("C:\\t\\apply-ics.ps1")),
        ];
        let mut problems = Vec::new();
        for (name, body) in scripts {
            let path = dir.join(name);
            std::fs::write(&path, &body).unwrap();
            let check = format!(
                "$t = $null; $e = $null; [void][System.Management.Automation.Language.Parser]::ParseFile('{}', [ref]$t, [ref]$e); if ($e) {{ $e | ForEach-Object {{ \"$($_.Extent.StartLineNumber): $($_.Message)\" }} }}",
                path.display()
            );
            let out = std::process::Command::new(&pwsh).args(["-NoProfile", "-Command", &check]).output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                problems.push(format!("{name}: {text}"));
            }
        }
        let _ = std::fs::remove_dir_all(dir);
        assert!(problems.is_empty(), "PowerShell parse errors:\n{}", problems.join("\n"));
    }

    #[test]
    fn ps_quote_doubles_quotes() {
        assert_eq!(ps_quote("a'b"), "'a''b'");
        assert_eq!(ps_quote("plain"), "'plain'");
    }
}
