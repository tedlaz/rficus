//! Accept a url dragged straight from a browser.
//!
//! winit registers its own OLE drop target and asks it only for `CF_HDROP`
//! (winit-0.30 `drop_handler.rs`), so every text or link drag is refused before
//! egui ever hears about it. This replaces that target with one that also reads
//! the formats a browser puts on a dragged link, and keeps handling files so
//! nothing is lost by displacing winit's.
//!
//! Drops arrive on the thread owning the window, which is the UI thread, so the
//! collected urls are just parked in a mutex for the next frame to drain.

use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{HGLOBAL, HWND, POINTL};
use windows::Win32::System::Com::{
    IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
    CF_HDROP, CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, CFSTR_INETURLW, HDROP};
use windows::core::{implement, Ref, Result as WinResult};

/// Urls waiting to be picked up by the next frame.
pub type Sink = Arc<Mutex<Vec<String>>>;

/// Every http(s) line in some text. A browser hands over one url, but a dropped
/// text file may hold a list.
pub fn urls_in_text(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim().trim_start_matches("URL="))
        .filter(|l| l.starts_with("http://") || l.starts_with("https://"))
        .map(str::to_owned)
        .collect()
}

#[implement(IDropTarget)]
struct Target {
    sink: Sink,
    repaint: Box<dyn Fn() + Send + Sync>,
}

impl Target {
    fn collect(&self, data: Option<&IDataObject>) -> Vec<String> {
        let Some(data) = data else { return Vec::new() };
        let mut urls = Vec::new();

        // A dragged link: the url itself, in the shell's own format.
        let url_format = unsafe { RegisterClipboardFormatW(CFSTR_INETURLW) } as u16;
        if url_format != 0 && let Some(text) = read_text(data, url_format) {
            urls.extend(urls_in_text(&text));
        }
        // Dragged selected text that happens to be a url.
        if urls.is_empty() && let Some(text) = read_text(data, CF_UNICODETEXT.0) {
            urls.extend(urls_in_text(&text));
        }
        // Files, which winit's target used to handle for us.
        if urls.is_empty() {
            for path in read_files(data) {
                // A dropped video is a file too: only read something that could
                // plausibly be a list of links, never a multi-gigabyte payload.
                let small = std::fs::metadata(&path).is_ok_and(|m| m.len() <= MAX_LIST_BYTES);
                if small && let Ok(bytes) = std::fs::read(&path) {
                    urls.extend(urls_in_text(&String::from_utf8_lossy(&bytes)));
                }
            }
        }
        urls
    }

    /// Is a format we understand on offer? Cheaper than fetching the payload,
    /// and this runs while the cursor is still moving over the window.
    fn accepts(&self, data: Option<&IDataObject>) -> bool {
        let Some(data) = data else { return false };
        let url_format = unsafe { RegisterClipboardFormatW(CFSTR_INETURLW) } as u16;
        [url_format, CF_UNICODETEXT.0, CF_HDROP.0]
            .into_iter()
            .filter(|f| *f != 0)
            .any(|f| unsafe { data.QueryGetData(&formatetc(f)) }.is_ok())
    }
}

/// A text file of links is small; anything larger is not a link list.
const MAX_LIST_BYTES: u64 = 1 << 20;

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> WinResult<()> {
        let accept = self.accepts(data.as_ref());
        unsafe { *effect = if accept { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }

    fn DragOver(
        &self,
        _keys: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> WinResult<()> {
        // DragEnter already decided; keep showing the same cursor.
        unsafe { *effect = DROPEFFECT_COPY };
        Ok(())
    }

    fn DragLeave(&self) -> WinResult<()> {
        Ok(())
    }

    fn Drop(
        &self,
        data: Ref<IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> WinResult<()> {
        let urls = self.collect(data.as_ref());
        unsafe { *effect = if urls.is_empty() { DROPEFFECT_NONE } else { DROPEFFECT_COPY } };
        if !urls.is_empty() {
            if let Ok(mut sink) = self.sink.lock() {
                sink.extend(urls);
            }
            (self.repaint)();
        }
        Ok(())
    }
}

fn formatetc(format: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

/// Read a NUL-terminated UTF-16 payload out of the drag's global memory.
fn read_text(data: &IDataObject, format: u16) -> Option<String> {
    let mut medium = unsafe { data.GetData(&formatetc(format)) }.ok()?;
    let text = unsafe { with_hglobal(medium.u.hGlobal, wide_to_string) };
    unsafe { ReleaseStgMedium(&mut medium) };
    text.filter(|t| !t.is_empty())
}

fn read_files(data: &IDataObject) -> Vec<std::path::PathBuf> {
    let Ok(mut medium) = (unsafe { data.GetData(&formatetc(CF_HDROP.0)) }) else {
        return Vec::new();
    };
    let paths = unsafe { hdrop_paths(HDROP(medium.u.hGlobal.0)) };
    unsafe { ReleaseStgMedium(&mut medium) };
    paths
}

/// Lock the handle for the duration of `read`, and always unlock.
unsafe fn with_hglobal<T>(handle: HGLOBAL, read: unsafe fn(*const u16) -> T) -> Option<T> {
    let ptr = unsafe { GlobalLock(handle) } as *const u16;
    if ptr.is_null() {
        return None;
    }
    let value = unsafe { read(ptr) };
    let _ = unsafe { GlobalUnlock(handle) };
    Some(value)
}

unsafe fn wide_to_string(ptr: *const u16) -> String {
    let mut len = 0usize;
    // Drag payloads are small; the terminator is what bounds this.
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) })
}

unsafe fn hdrop_paths(hdrop: HDROP) -> Vec<std::path::PathBuf> {
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    (0..count)
        .filter_map(|i| {
            let len = unsafe { DragQueryFileW(hdrop, i, None) } as usize;
            if len == 0 {
                return None;
            }
            let mut buf = vec![0u16; len + 1];
            unsafe { DragQueryFileW(hdrop, i, Some(&mut buf)) };
            buf.pop();
            Some(std::path::PathBuf::from(String::from_utf16_lossy(&buf)))
        })
        .collect()
}

/// Keeps the drop target registered for as long as it is alive.
pub struct Dnd {
    hwnd: HWND,
    pub sink: Sink,
    _target: IDropTarget,
}

impl Dnd {
    /// Take over drag and drop for this window. Fails harmlessly: without it
    /// the app simply does not accept drops, and Ctrl+V still works.
    pub fn install(hwnd: isize, repaint: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        let hwnd = HWND(hwnd as *mut std::ffi::c_void);
        let sink: Sink = Arc::default();
        let target: IDropTarget = Target {
            sink: sink.clone(),
            repaint: Box::new(repaint),
        }
        .into();

        unsafe {
            // winit got there first, and its target refuses everything but files.
            let _ = RevokeDragDrop(hwnd);
            RegisterDragDrop(hwnd, &target).ok()?;
        }
        Some(Self {
            hwnd,
            sink,
            _target: target,
        })
    }

    /// Hand over anything dropped since the last frame.
    pub fn take(&self) -> Vec<String> {
        self.sink
            .lock()
            .map(|mut s| std::mem::take(&mut *s))
            .unwrap_or_default()
    }
}

impl Drop for Dnd {
    fn drop(&mut self) {
        unsafe { let _ = RevokeDragDrop(self.hwnd); };
    }
}

#[cfg(test)]
mod tests {
    use super::urls_in_text;

    #[test]
    fn picks_urls_out_of_dropped_text() {
        // What a browser hands over for a dragged link.
        assert_eq!(
            urls_in_text("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            vec!["https://www.youtube.com/watch?v=dQw4w9WgXcQ"]
        );
        // A .url shortcut file.
        let shortcut = "[InternetShortcut]\r\nURL=https://example.com/v\r\n";
        assert_eq!(urls_in_text(shortcut), vec!["https://example.com/v"]);
        // A list, with noise around it.
        let list = "  https://a.test/1  \nnot a url\nhttps://b.test/2\n";
        assert_eq!(urls_in_text(list), vec!["https://a.test/1", "https://b.test/2"]);
        // Nothing droppable.
        assert!(urls_in_text("ftp://a.test/x\nplain words").is_empty());
    }
}

#[cfg(test)]
mod com_tests {
    use super::*;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, STGMEDIUM, STGMEDIUM_0};
    use windows::Win32::System::Memory::{GlobalAlloc, GMEM_MOVEABLE};
    use windows::Win32::UI::Shell::SHCreateDataObject;

    /// A real IDataObject holding `text` under `format`, built the way the
    /// shell builds one, so the COM read path is exercised for real: FORMATETC
    /// matching, GetData, HGLOBAL locking and the UTF-16 decode.
    fn data_object(format: u16, text: &str) -> IDataObject {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = std::mem::size_of_val(&wide[..]);
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes).unwrap();
            let dst = GlobalLock(handle) as *mut u16;
            std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
            let _ = GlobalUnlock(handle);

            let obj: IDataObject = SHCreateDataObject(None, None, None).unwrap();
            let medium = STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: handle },
                pUnkForRelease: std::mem::ManuallyDrop::new(None),
            };
            // The object takes ownership, so it frees the handle.
            obj.SetData(&formatetc(format), &medium, true).unwrap();
            obj
        }
    }

    #[test]
    fn reads_a_dragged_link_out_of_a_data_object() {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };

        let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        let url_format = unsafe { RegisterClipboardFormatW(CFSTR_INETURLW) } as u16;

        // What a browser puts on a dragged link.
        let obj = data_object(url_format, url);
        assert_eq!(read_text(&obj, url_format).as_deref(), Some(url));

        // Dragged plain text.
        let obj = data_object(CF_UNICODETEXT.0, url);
        assert_eq!(read_text(&obj, CF_UNICODETEXT.0).as_deref(), Some(url));

        // A format the payload does not carry is simply absent, not a crash.
        assert_eq!(read_text(&obj, url_format), None);
    }
}
