//! Progress on the Windows taskbar button, so a minimised window still reports.

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList, TBPF_NOPROGRESS, TBPF_NORMAL};

pub struct Taskbar {
    list: ITaskbarList3,
    hwnd: HWND,
    /// Whole percent last pushed, or -1 for "no bar". Nothing is sent twice:
    /// this runs on every repaint of a download.
    last: i32,
}

impl Taskbar {
    /// Fails harmlessly: without it the app simply shows no taskbar progress.
    pub fn new(hwnd: isize) -> Option<Self> {
        // COM is already initialised on this thread by the drop target.
        let list: ITaskbarList3 = unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_ALL) }.ok()?;
        Some(Self {
            list,
            hwnd: HWND(hwnd as *mut std::ffi::c_void),
            last: -1,
        })
    }

    /// `None` clears the bar.
    pub fn set(&mut self, percent: Option<f32>) {
        let value = percent.map_or(-1, |p| p.clamp(0.0, 100.0) as i32);
        if value == self.last {
            return;
        }
        self.last = value;
        unsafe {
            if value < 0 {
                let _ = self.list.SetProgressState(self.hwnd, TBPF_NOPROGRESS);
            } else {
                let _ = self.list.SetProgressState(self.hwnd, TBPF_NORMAL);
                let _ = self.list.SetProgressValue(self.hwnd, value as u64, 100);
            }
        }
    }
}
