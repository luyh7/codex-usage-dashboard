#[cfg(windows)]
fn main() {
    use muda::{Menu, MenuItem, MenuTheme, Submenu};
    use std::{io, ptr::null_mut};
    use windows_sys::{
        Win32::{
            Foundation::STATUS_ACCESS_VIOLATION,
            System::{
                Diagnostics::Debug::{
                    AddVectoredExceptionHandler, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS,
                },
                Threading::{GetCurrentProcess, TerminateProcess},
            },
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, SendMessageW, WM_NCPAINT, WS_OVERLAPPEDWINDOW,
            },
        },
        w,
    };

    unsafe extern "system" fn fail_on_access_violation(info: *mut EXCEPTION_POINTERS) -> i32 {
        let code = unsafe { (*(*info).ExceptionRecord).ExceptionCode };
        if code == STATUS_ACCESS_VIOLATION {
            unsafe { TerminateProcess(GetCurrentProcess(), code as u32) };
        }
        EXCEPTION_CONTINUE_SEARCH
    }

    let item = MenuItem::new("Quit", true, None);
    let submenu = Submenu::with_items("CUD", true, &[&item]).unwrap();
    let menu = Menu::with_items(&[&submenu]).unwrap();

    // Paint a hidden native menu in dark mode regardless of the system theme.
    unsafe {
        let window = CreateWindowExW(
            0,
            w!("STATIC"),
            w!("CUD dark menu test"),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            300,
            200,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
        );
        assert!(
            !window.is_null(),
            "CreateWindowExW: {}",
            io::Error::last_os_error()
        );

        menu.init_for_hwnd_with_theme(window as isize, MenuTheme::Dark)
            .unwrap();
        SendMessageW(window, WM_NCPAINT, 1, 0);

        menu.remove_for_hwnd(window as isize).unwrap();
        assert_ne!(
            DestroyWindow(window),
            0,
            "DestroyWindow: {}",
            io::Error::last_os_error()
        );
    }
    drop(menu);

    // Windows can keep the original exit code after reporting an exception during shutdown.
    let handler = unsafe { AddVectoredExceptionHandler(1, Some(fail_on_access_violation)) };
    assert!(
        !handler.is_null(),
        "AddVectoredExceptionHandler: {}",
        io::Error::last_os_error()
    );

    std::process::exit(0);
}

#[cfg(not(windows))]
fn main() {}
