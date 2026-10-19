//! Running PowerShell from the app: hidden (no window) for reads, and
//! elevated through one UAC prompt for the few things Windows protects
//! (hosts file, Internet Connection Sharing).

/// Single-quoted PowerShell string literal.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(windows)]
fn scratch_dir() -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join("hotspot-pool");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[cfg(windows)]
fn powershell(args: &[&str]) -> Result<std::process::Output, String> {
    // CREATE_NO_WINDOW: a console app would otherwise flash a window before
    // -WindowStyle Hidden takes effect.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    use std::os::windows::process::CommandExt;
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden"])
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("Could not launch PowerShell: {e}"))
}

/// Run `body` in a hidden, non-elevated PowerShell and return its stdout.
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn run_hidden(body: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        let script = scratch_dir()?.join(format!("hidden-{}.ps1", std::process::id()));
        std::fs::write(&script, body).map_err(|e| e.to_string())?;
        let out = powershell(&["-ExecutionPolicy", "Bypass", "-File", &script.display().to_string()])?;
        let _ = std::fs::remove_file(&script);
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
    #[cfg(not(windows))]
    {
        Err("Not supported on this OS".into())
    }
}

/// Write `body` to `%TEMP%\hotspot-pool\<script_name>` and run it elevated
/// (one UAC prompt), waiting for it to finish.
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn run_elevated(script_name: &str, body: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let script = scratch_dir()?.join(script_name);
        std::fs::write(&script, body).map_err(|e| e.to_string())?;
        // Outer (non-elevated) PowerShell launches an elevated one and waits for it.
        let arg = format!(
            "Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','\"{}\"'",
            script.display()
        );
        let out = powershell(&["-Command", &arg])?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            if err.contains("canceled") || err.contains("cancelled") || err.contains("1223") {
                return Err("The Windows permission prompt was cancelled.".into());
            }
            return Err(err.trim().to_string());
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
    fn ps_quote_doubles_quotes() {
        assert_eq!(ps_quote("a'b"), "'a''b'");
        assert_eq!(ps_quote("plain"), "'plain'");
    }
}
