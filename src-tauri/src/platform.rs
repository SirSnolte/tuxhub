//! What differs between Linux and Windows below the hub's own logic: file
//! permissions, finding and ending processes, autostart at login and the
//! owner of a named pipe. Each function has one implementation per system.

use std::path::Path;

/// Whether the file `path` with metadata `md` can be started as a program.
#[cfg(unix)]
pub fn is_executable(_path: &Path, md: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    md.is_file() && md.permissions().mode() & 0o111 != 0
}

/// Whether the file `path` can be started as a program: Windows decides by
/// the file type.
#[cfg(windows)]
pub fn is_executable(path: &Path, md: &std::fs::Metadata) -> bool {
    md.is_file() && has_program_extension(path)
}

/// Whether `path` names a program Windows starts by itself.
#[cfg(any(windows, test))]
pub fn has_program_extension(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        ["exe", "com", "bat", "cmd"]
            .iter()
            .any(|x| e.eq_ignore_ascii_case(x))
    })
}

/// Marks a file as a program (Linux); Windows needs nothing for that.
pub fn make_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
    }
    #[cfg(windows)]
    {
        let _ = path;
        Ok(())
    }
}

/// Starts a program in its own process group, so it keeps running when
/// tuxhub exits; on Windows also without a console window.
pub fn detach(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(unix)]
    {
        cmd.process_group(0)
    }
    #[cfg(windows)]
    {
        /// `CREATE_NEW_PROCESS_GROUP`.
        const NEW_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(NEW_GROUP | NO_WINDOW)
    }
}

/// Processes of the current user and the paths of their programs.
#[cfg(windows)]
pub fn process_paths() -> Vec<(u32, std::path::PathBuf)> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };

    let mut out = Vec::new();
    // SAFETY: plain Win32 calls; every handle opened here is closed here, and
    // the buffers outlive the calls that fill them.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snap, &mut entry) != 0;
        while more {
            let pid = entry.th32ProcessID;
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            // Processes of other users and of the system cannot be opened.
            if !process.is_null() {
                let mut buf = [0u16; 1024];
                let mut len = buf.len() as u32;
                if QueryFullProcessImageNameW(
                    process,
                    PROCESS_NAME_WIN32,
                    buf.as_mut_ptr(),
                    &mut len,
                ) != 0
                    && same_user(process)
                {
                    let path = std::ffi::OsString::from_wide(&buf[..len as usize]);
                    out.push((pid, path.into()));
                }
                CloseHandle(process);
            }
            more = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
    }
    out
}

/// The security identifier of the account a process runs as.
#[cfg(windows)]
unsafe fn account_of(process: windows_sys::Win32::Foundation::HANDLE) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetLengthSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::System::Threading::OpenProcessToken;

    // SAFETY: the caller passes a valid process handle; the token handle is
    // closed below and the SID is copied out of the buffer before it is freed.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        // u64 elements keep the buffer aligned for the pointers in TOKEN_USER.
        let mut buf = vec![0u64; 64];
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenUser,
            buf.as_mut_ptr().cast(),
            (buf.len() * 8) as u32,
            &mut len,
        ) != 0;
        CloseHandle(token);
        if !ok {
            return None;
        }
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let sid = user.User.Sid;
        let n = GetLengthSid(sid) as usize;
        Some(std::slice::from_raw_parts(sid as *const u8, n).to_vec())
    }
}

/// Whether `process` runs as the same account as tuxhub.
#[cfg(windows)]
unsafe fn same_user(process: windows_sys::Win32::Foundation::HANDLE) -> bool {
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: both handles are valid process handles.
    unsafe {
        let own = account_of(GetCurrentProcess());
        own.is_some() && own == account_of(process)
    }
}

/// Whether the server of a connected named pipe runs as the current user.
/// Any user can create a pipe of any name, so another user could pose as a
/// tool; such a connection is dropped.
#[cfg(windows)]
pub fn pipe_server_is_own_user(pipe: &tokio::net::windows::named_pipe::NamedPipeClient) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    // SAFETY: the pipe handle is valid while `pipe` lives; the process handle
    // is closed here.
    unsafe {
        let mut pid = 0u32;
        if GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) == 0 {
            return false;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return false;
        }
        let own = same_user(process);
        CloseHandle(process);
        own
    }
}

/// Asks processes to end (Windows: a close message to their windows, as
/// when the user closes them), or ends them at once with `force`.
#[cfg(windows)]
pub async fn end_processes(pids: &[u32], force: bool) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new("taskkill");
    if force {
        cmd.arg("/F");
    }
    for pid in pids {
        cmd.arg("/PID").arg(pid.to_string());
    }
    // A process may exit in between, so the exit status does not matter.
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(NO_WINDOW)
        .status()
        .await
        .map(|_| ())
        .map_err(|e| format!("could not run taskkill: {e}"))
}

/// `CREATE_NO_WINDOW`: console programs started by tuxhub open no window.
#[cfg(windows)]
pub const NO_WINDOW: u32 = 0x0800_0000;

/// The registry key of programs Windows starts at login.
#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// The value of tuxhub below [`RUN_KEY`].
#[cfg(windows)]
const RUN_VALUE: &str = "tuxhub";

/// The command line Windows runs at login: the program in quotes, so paths
/// with spaces work, and the arguments after it.
#[cfg(any(windows, test))]
pub fn run_command(exe: &Path, args: &[&str]) -> String {
    let mut s = format!("\"{}\"", exe.display());
    for a in args {
        s.push(' ');
        s.push_str(a);
    }
    s
}

/// Starts tuxhub at login with `args`, or no longer (`None`). Only tuxhub's
/// own value is written or removed.
#[cfg(windows)]
pub fn set_autostart(command: Option<&str>) -> Result<(), String> {
    let key = windows_registry::CURRENT_USER
        .create(RUN_KEY)
        .map_err(|e| format!("autostart: {e}"))?;
    match command {
        Some(c) => key
            .set_string(RUN_VALUE, c)
            .map_err(|e| format!("autostart: {e}")),
        None => match key.remove_value(RUN_VALUE) {
            Ok(()) => Ok(()),
            // ERROR_FILE_NOT_FOUND: there was no entry.
            Err(e) if e.code().0 as u32 == 0x8007_0002 => Ok(()),
            Err(e) => Err(format!("autostart: {e}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_command_quotes_the_program() {
        let exe = Path::new(r"C:\Users\Jo Doe\AppData\Local\tuxhub\tuxhub.exe");
        assert_eq!(
            run_command(exe, &["--minimized"]),
            r#""C:\Users\Jo Doe\AppData\Local\tuxhub\tuxhub.exe" --minimized"#
        );
        assert_eq!(run_command(exe, &[]), format!("\"{}\"", exe.display()));
    }

    #[test]
    fn program_extensions() {
        assert!(has_program_extension(Path::new("Fliq.EXE")));
        assert!(has_program_extension(Path::new("run.cmd")));
        assert!(!has_program_extension(Path::new("readme.txt")));
        assert!(!has_program_extension(Path::new("fliq")));
    }
}
