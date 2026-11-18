//! Bind cancellation to the process serving this task's COM application window.
#[cfg(windows)]
use std::{collections::HashSet, os::windows::io::OwnedHandle, sync::Mutex};

pub struct KillTracker {
  #[cfg(windows)]
  existing: Option<HashSet<u32>>,
  #[cfg(windows)]
  process: Mutex<Option<(u32, OwnedHandle)>>,
}

impl KillTracker {
  pub fn capture() -> Self {
    Self {
      #[cfg(windows)]
      existing: snapshot_pids(),
      #[cfg(windows)]
      process: Mutex::new(None),
    }
  }

  #[cfg(windows)]
  pub fn track_window(&self, hwnd: windows::Win32::Foundation::HWND) {
    use std::os::windows::io::FromRawHandle;
    use windows::Win32::{
      System::Threading::{OpenProcess, PROCESS_TERMINATE},
      UI::WindowsAndMessaging::GetWindowThreadProcessId,
    };
    let Some(existing) = &self.existing else {
      return;
    };
    let mut pid = 0;
    unsafe {
      if GetWindowThreadProcessId(hwnd, Some(&mut pid)) == 0 || existing.contains(&pid) {
        return;
      }
      if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
        let owned = OwnedHandle::from_raw_handle(handle.0 as _);
        // Check the window again after opening the handle in case the process exited.
        let mut confirmed = 0;
        if GetWindowThreadProcessId(hwnd, Some(&mut confirmed)) != 0 && confirmed == pid {
          *self.process.lock().unwrap() = Some((pid, owned));
        }
      }
    }
  }

  pub fn terminate_owned_process(&self) -> Option<u32> {
    #[cfg(windows)]
    {
      use std::os::windows::io::AsRawHandle;
      use windows::Win32::{Foundation::HANDLE, System::Threading::TerminateProcess};
      let (pid, owned) = self.process.lock().unwrap().take()?;
      unsafe { TerminateProcess(HANDLE(owned.as_raw_handle() as isize), 1).ok()? };
      Some(pid)
    }
    #[cfg(not(windows))]
    {
      None
    }
  }
}

#[cfg(windows)]
fn snapshot_pids() -> Option<HashSet<u32>> {
  use std::os::windows::io::{AsRawHandle, FromRawHandle};
  use windows::Win32::Foundation::{ERROR_NO_MORE_FILES, HANDLE};
  use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
  };
  unsafe {
    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
    let owned = OwnedHandle::from_raw_handle(snapshot.0 as _);
    let handle = HANDLE(owned.as_raw_handle() as isize);
    let mut entry = PROCESSENTRY32W {
      dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
      ..Default::default()
    };
    let mut pids = HashSet::new();
    Process32FirstW(handle, &mut entry).ok()?;
    loop {
      pids.insert(entry.th32ProcessID);
      if let Err(err) = Process32NextW(handle, &mut entry) {
        return (err.code() == ERROR_NO_MORE_FILES.to_hresult()).then_some(pids);
      }
    }
  }
}
