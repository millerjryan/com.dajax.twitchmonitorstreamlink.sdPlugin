/// Transient "Starting Streamlink..." dialog shown while the process is being spawned.
///
/// Windows-only implementation using raw WinAPI. On non-Windows targets the type
/// compiles as a no-op so call-sites need no cfg guards.

// ── Windows implementation ─────────────────────────────────────────────────

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::{
        atomic::{AtomicIsize, Ordering},
        Arc, Condvar, Mutex,
    };

    use winapi::shared::windef::HWND;
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winapi::um::winuser::*;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    /// Custom window procedure: forwards everything to DefWindowProcW, but posts
    /// WM_QUIT on WM_DESTROY so the message loop in the dialog thread exits.
    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        if msg == WM_DESTROY {
            PostQuitMessage(0);
            return 0;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    pub struct StartingDialog {
        hwnd: Arc<AtomicIsize>,
        shown_at: std::time::Instant,
    }

    impl StartingDialog {
        /// Spawn a background OS thread, create the window, then block until
        /// the HWND is ready before returning. This guarantees close() can
        /// always post to a valid window handle.
        pub fn show() -> Self {
            let hwnd_atom = Arc::new(AtomicIsize::new(0));
            let hwnd_clone = hwnd_atom.clone();

            // Condvar used to signal the caller once the window is created.
            let ready = Arc::new((Mutex::new(false), Condvar::new()));
            let ready_clone = ready.clone();

            std::thread::spawn(move || unsafe {
                let class_name = wide("StreamlinkStartDlg");
                let hinstance = GetModuleHandleW(std::ptr::null());

                // Register window class (ignore error if already registered).
                let mut wc: WNDCLASSEXW = std::mem::zeroed();
                wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
                wc.lpfnWndProc = Some(wnd_proc);
                wc.hInstance = hinstance;
                wc.hbrBackground = (COLOR_BTNFACE + 1) as *mut _;
                wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
                wc.lpszClassName = class_name.as_ptr();
                RegisterClassExW(&wc);

                // Centre the window on the primary monitor.
                let sw = GetSystemMetrics(SM_CXSCREEN);
                let sh = GetSystemMetrics(SM_CYSCREEN);
                let w: i32 = 300;
                let h: i32 = 90;
                let x = (sw - w) / 2;
                let y = (sh - h) / 2;

                let hwnd = CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                    class_name.as_ptr(),
                    wide("").as_ptr(),
                    WS_POPUP | WS_BORDER,
                    x, y, w, h,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null_mut(),
                );

                if hwnd.is_null() {
                    // Signal ready even on failure so the caller is not blocked.
                    let (lock, cvar) = &*ready_clone;
                    *lock.lock().unwrap() = true;
                    cvar.notify_one();
                    return;
                }

                // Static text child centred inside the popup.
                let static_class = wide("STATIC");
                let label = wide("Starting Streamlink...");
                CreateWindowExW(
                    0,
                    static_class.as_ptr(),
                    label.as_ptr(),
                    WS_CHILD | WS_VISIBLE | SS_CENTER,
                    0, 28, w, 30,
                    hwnd,
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null_mut(),
                );

                ShowWindow(hwnd, SW_SHOW);
                UpdateWindow(hwnd);

                // Publish the HWND, then unblock the caller.
                hwnd_clone.store(hwnd as isize, Ordering::SeqCst);
                {
                    let (lock, cvar) = &*ready_clone;
                    *lock.lock().unwrap() = true;
                    cvar.notify_one();
                }

                // Run the message loop for this thread until WM_QUIT.
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            });

            // Block until the background thread has created and shown the window.
            let (lock, cvar) = &*ready;
            let mut guard = lock.lock().unwrap();
            while !*guard {
                guard = cvar.wait(guard).unwrap();
            }

            Self { hwnd: hwnd_atom, shown_at: std::time::Instant::now() }
        }

        /// Close the dialog, but keep it visible for at least 3 seconds from
        /// when it was shown. Spawns a thread for the wait so this returns
        /// immediately.
        pub fn close(self) {
            let hwnd_val = self.hwnd.load(Ordering::SeqCst);
            if hwnd_val != 0 {
                let min_display = std::time::Duration::from_secs(3);
                let remaining = min_display.saturating_sub(self.shown_at.elapsed());
                std::thread::spawn(move || {
                    if !remaining.is_zero() {
                        std::thread::sleep(remaining);
                    }
                    unsafe {
                        PostMessageW(hwnd_val as HWND, WM_CLOSE, 0, 0);
                    }
                });
            }
        }
    }
}

// ── Non-Windows stub ───────────────────────────────────────────────────────

#[cfg(not(windows))]
mod imp {
    pub struct StartingDialog;
    impl StartingDialog {
        pub fn show() -> Self { Self }
        pub fn close(self) {}
    }
}

// ── Public re-export ───────────────────────────────────────────────────────

pub use imp::StartingDialog;
