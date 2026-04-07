use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{EXCEPTION_ACCESS_VIOLATION, EXCEPTION_STACK_OVERFLOW},
    System::Diagnostics::Debug::{AddVectoredExceptionHandler, EXCEPTION_POINTERS},
};

struct CrashFile(Mutex<File>);

impl std::io::Write for CrashFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.get_mut().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.get_mut().unwrap().flush()
    }
}

impl log::Log for CrashFile {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let msg = format!("[{:5}] {}\n", record.level(), record.args());
            let _ = self.0.lock().unwrap().write_all(msg.as_bytes());
        }
    }
    fn flush(&self) {
        let _ = self.0.lock().unwrap().flush();
    }
}

fn log_file_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    exe_dir.join("tonedock_crash.log")
}

fn write_crash_header(path: &std::path::Path) {
    if let Ok(mut f) = OpenOptions::new().append(true).open(path) {
        let _ = writeln!(
            f,
            "\n========== ToneDock started at {} ==========",
            chrono_free_timestamp()
        );
    }
}

fn chrono_free_timestamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;
    let year = 1970 + (days / 365);
    let remaining = days % 365;
    format!(
        "{:04}-{:03}T{:02}:{:02}:{:02} (unix {})",
        year, remaining, h, m, s, secs
    )
}

pub fn init() {
    let path = log_file_path();

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .expect("cannot open crash log file");

    write_crash_header(&path);

    let crash_file = Box::new(CrashFile(Mutex::new(file)));
    let crash_file_ptr = Box::into_raw(crash_file) as *mut CrashFile;

    unsafe {
        log::set_boxed_logger(Box::from_raw(crash_file_ptr)).expect("failed to set logger");
        log::set_max_level(log::LevelFilter::Info);
    }

    let log_path = path.clone();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("\n!!! PANIC at {} !!!\n{}\n", chrono_free_timestamp(), info);
        eprintln!("{}", msg);
        if let Ok(mut f) = OpenOptions::new().append(true).open(&log_path) {
            let _ = f.write_all(msg.as_bytes());
            let _ = f.write_all(b"\nBacktrace:\n");
            let bt = std::backtrace::Backtrace::capture();
            let _ = f.write_all(format!("{:?}\n", bt).as_bytes());
            let _ = f.flush();
        }
    }));

    #[cfg(target_os = "windows")]
    {
        unsafe {
            AddVectoredExceptionHandler(1, Some(vecored_exception_handler));
        }
    }

    log::info!("Crash log file: {}", path.display());
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn vecored_exception_handler(
    exception_info: *mut EXCEPTION_POINTERS,
) -> i32 {
    unsafe {
        if exception_info.is_null() {
            return 1;
        }

        let record = (*exception_info).ExceptionRecord;
        if record.is_null() {
            return 1;
        }

        let code = (*record).ExceptionCode;

        if code == EXCEPTION_ACCESS_VIOLATION {
            let addr = (*record).ExceptionInformation[1];
            let flags = (*record).ExceptionInformation[0];
            let op = if flags == 0 {
                "read"
            } else if flags == 1 {
                "write"
            } else if flags == 8 {
                "DEP"
            } else {
                "unknown"
            };

            let msg = format!(
                "\n!!! NATIVE CRASH: ACCESS VIOLATION !!!\n\
                 Code: 0x{:08X} (Access Violation)\n\
                 Operation: {} at address 0x{:016X}\n\
                 Time: {}\n\
                 This is likely caused by a VST3 plugin calling into invalid memory.\n",
                code,
                op,
                addr,
                chrono_free_timestamp(),
            );
            eprintln!("{}", msg);
            if let Ok(mut f) = OpenOptions::new().append(true).open(log_file_path()) {
                let _ = f.write_all(msg.as_bytes());
                let _ = f.write_all(b"Stack trace not available for native crashes.\n");
                let _ = f.flush();
            }
        } else if code == EXCEPTION_STACK_OVERFLOW {
            let msg = format!(
                "\n!!! NATIVE CRASH: STACK OVERFLOW !!!\n\
                 Code: 0x{:08X}\n\
                 Time: {}\n",
                code,
                chrono_free_timestamp(),
            );
            eprintln!("{}", msg);
            if let Ok(mut f) = OpenOptions::new().append(true).open(log_file_path()) {
                let _ = f.write_all(msg.as_bytes());
                let _ = f.flush();
            }
        }
    }

    1
}
