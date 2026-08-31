//! Descendant snapshots: the OS process table as observed right now.
//!
//! Windows walks the Toolhelp snapshot (windows-sys, already linked for the
//! Job Object). POSIX runs `ps -axo pid=,ppid=,pgid=` with a 32 MB output
//! cap, a 1 s timeout, and `LC_ALL=C` so parsers never see localized output.
//! A snapshot is advisory: it is only ever used to aim a kill at children
//! captured together with the parent's pgid.

/// Hard cap on captured `ps` output: 32 MB.
#[cfg_attr(not(unix), allow(dead_code))]
const PS_CAPTURE_CAP: usize = 32 * 1024 * 1024;

/// `ps` must answer within 1 s or the snapshot is abandoned.
#[cfg_attr(not(unix), allow(dead_code))]
const PS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

/// Returns the direct + transitive children of `pid` at snapshot time.
#[allow(unused_variables)]
pub fn list_descendants(pid: u32) -> Vec<u32> {
    #[cfg(windows)]
    {
        windows_descendants(pid)
    }
    #[cfg(not(windows))]
    {
        posix_descendants(pid)
    }
}

#[cfg(windows)]
fn windows_descendants(pid: u32) -> Vec<u32> {
    use std::collections::BTreeMap;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }

    let mut rows: BTreeMap<u32, u32> = BTreeMap::new();
    unsafe {
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                rows.insert(entry.th32ProcessID, entry.th32ParentProcessID);
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }

    // BFS down the parent links.
    let mut descendants = Vec::new();
    let mut frontier = vec![pid];
    while let Some(parent) = frontier.pop() {
        for (child_pid, parent_pid) in &rows {
            if *parent_pid == parent && *child_pid != pid {
                descendants.push(*child_pid);
                frontier.push(*child_pid);
            }
        }
    }
    descendants.sort_unstable();
    descendants.dedup();
    descendants
}

#[cfg(not(windows))]
fn posix_descendants(pid: u32) -> Vec<u32> {
    use std::collections::BTreeMap;
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let Ok(mut child) = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,pgid="])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };

    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        return Vec::new();
    };

    let mut buf = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let started = Instant::now();
    loop {
        match stdout.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > PS_CAPTURE_CAP {
                    break;
                }
            }
            Err(_) => break,
        }
        if started.elapsed() >= PS_TIMEOUT {
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    let text = String::from_utf8_lossy(&buf);
    let mut parent_of: BTreeMap<u32, u32> = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            continue;
        }
        if let (Ok(child_pid), Ok(parent_pid)) =
            (fields[0].parse::<u32>(), fields[1].parse::<u32>())
        {
            parent_of.insert(child_pid, parent_pid);
        }
    }

    let mut descendants = Vec::new();
    let mut frontier = vec![pid];
    while let Some(parent) = frontier.pop() {
        for (child_pid, parent_pid) in &parent_of {
            if *parent_pid == parent && *child_pid != pid {
                descendants.push(*child_pid);
                frontier.push(*child_pid);
            }
        }
    }
    descendants.sort_unstable();
    descendants.dedup();
    descendants
}
