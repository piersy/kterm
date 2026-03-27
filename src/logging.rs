use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

static LOG_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Initialize the file logger. Call once at startup.
pub fn init() {
    if let Ok(file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("kterm.log")
    {
        if let Ok(mut guard) = LOG_FILE.lock() {
            *guard = Some(file);
        }
    }
}

/// Log an error message to kterm.log with a timestamp.
pub fn log_error(msg: &str) {
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(ref mut file) = *guard {
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(file, "[{}] ERROR: {}", now, msg);
            let _ = file.flush();
        }
    }
}

/// Log an info message to kterm.log with a timestamp.
#[allow(dead_code)]
pub fn log_info(msg: &str) {
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(ref mut file) = *guard {
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(file, "[{}] INFO: {}", now, msg);
            let _ = file.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_log_error_writes_to_file() {
        // Use a temp file for testing
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_owned();

        {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();
            let mut guard = LOG_FILE.lock().unwrap();
            *guard = Some(file);
        }

        log_error("test error message");

        {
            let mut guard = LOG_FILE.lock().unwrap();
            *guard = None; // close file
        }

        let mut contents = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();

        assert!(contents.contains("ERROR: test error message"));
        assert!(contents.contains("] ERROR:"));
    }

    #[test]
    fn test_log_info_writes_to_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_owned();

        {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();
            let mut guard = LOG_FILE.lock().unwrap();
            *guard = Some(file);
        }

        log_info("test info message");

        {
            let mut guard = LOG_FILE.lock().unwrap();
            *guard = None;
        }

        let mut contents = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();

        assert!(contents.contains("INFO: test info message"));
    }
}
