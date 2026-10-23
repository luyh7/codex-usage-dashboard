//! Diagnostic for WSL metadata calls; takes only an explicit synthetic directory.
#[cfg(target_os = "linux")]
fn main() {
    use std::{ffi::CString, mem::MaybeUninit, os::unix::ffi::OsStrExt, time::Instant};
    let paths: Vec<_> = std::fs::read_dir(std::env::args_os().nth(1).unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    for mode in ["metadata", "stat", "fstatat", "inventory-stamp"] {
        let start = Instant::now();
        for path in &paths {
            match mode {
                "metadata" => {
                    std::hint::black_box(std::fs::metadata(path).unwrap());
                }
                "inventory-stamp" => {
                    std::hint::black_box(usage_core::inventory::FileStamp::read(path).unwrap());
                }
                _ => {
                    let path = CString::new(path.as_os_str().as_bytes()).unwrap();
                    let mut stat = MaybeUninit::<libc::stat>::uninit();
                    // SAFETY: correct buffers and NUL-terminated input; only the return code is read.
                    let result = unsafe {
                        if mode == "stat" {
                            libc::stat(path.as_ptr(), stat.as_mut_ptr())
                        } else {
                            libc::fstatat(libc::AT_FDCWD, path.as_ptr(), stat.as_mut_ptr(), 0)
                        }
                    };
                    assert_eq!(result, 0);
                }
            }
        }
        println!(
            "{mode} {} {}",
            paths.len(),
            start.elapsed().as_secs_f64() * 1000.
        );
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {}
