use std::mem::MaybeUninit;
use std::path::Path;
use rustix::fs;
use rustix::io::Errno;
use serde::Serialize;

// JSON Report Struct
#[derive(Serialize)]
pub(crate) struct Report {
    user_cpu_time_secs: f64,
    sys_cpu_time_secs: f64,
    max_rss_kb: i64,
    major_page_faults: i64,
    minor_page_faults: i64,
    block_input_ops: i64,
    block_output_ops: i64,
    voluntary_context_switches: i64,
    involuntary_context_switches: i64
}

impl Report {
    // Call `getrusage` and initialize the struct.
    // Instructions: To be used in parent process.
    pub fn new() -> Result<Self, Errno> {
        // Create a zeroed `libc::rusage` struct
        let mut rusage: libc::rusage = unsafe {
            MaybeUninit::zeroed().assume_init()
        };

        // Call `getrusage` and fill the struct
        let res = unsafe {
            libc::getrusage(libc::RUSAGE_CHILDREN, &mut rusage)
        };

        // Return Errno if call failed
        if res == -1 {
            let raw_err = unsafe { *libc::__errno_location() };
            let err = Errno::from_raw_os_error(raw_err);
            eprintln!("[ERROR]: Failed to retrieve resource usage metrics from the kernel: {err}");
            return Err(err);
        }

        // Initialize the struct and return it
        Ok(Self {
            user_cpu_time_secs: rusage.ru_utime.tv_sec as f64
                                + (rusage.ru_utime.tv_usec as f64 / 1_000_000.0),

            sys_cpu_time_secs: rusage.ru_stime.tv_sec as f64 
                                + (rusage.ru_stime.tv_usec as f64 / 1_000_000.0),

            max_rss_kb: rusage.ru_maxrss,
            major_page_faults: rusage.ru_majflt,
            minor_page_faults: rusage.ru_minflt,
            block_input_ops: rusage.ru_inblock,
            block_output_ops: rusage.ru_oublock,
            voluntary_context_switches: rusage.ru_nvcsw,
            involuntary_context_switches: rusage.ru_nivcsw,
        })
    }

    // Print resource usage in output stream (terminal)
    pub fn print_resource_usage(&self) {
        eprintln!();
        eprintln!("--- Resource Usage ---");
        eprintln!("User CPU Time: {}", self.user_cpu_time_secs);
        eprintln!("System CPU Time: {}", self.sys_cpu_time_secs);
        eprintln!("Max RSS (Memory): {} KB", self.max_rss_kb);
        eprintln!("Major Page Faults: {}", self.major_page_faults);
        eprintln!("----------------------");
        eprintln!();
    }

    // Export a clean and detailed JSON report
    pub fn export_resource_usage(&self) -> Result<(), Errno> {
        let dir_path = Path::new("output");
        let file_path = Path::new("output/usage_report.json");

        // Create the output directory relative to the CWD
        // `0o755` translates to `rwx` permissions for owner and `rx` for others (rwxr-xr-x).
        match fs::mkdirat(fs::CWD, dir_path, fs::Mode::from_raw_mode(0o755)) {
            Ok(_) => {}, // Directory created successfully
            Err(Errno::EXIST) => {}, // Directory already exists, proceed
            Err(e) => {
                eprintln!("[ERROR]: Failed to create output directory: {e}");
                return Err(e);
            }
        } 

        // Open / create the JSON file
        let oflags = fs::OFlags::CREATE | fs::OFlags::WRONLY | fs::OFlags::TRUNC;
        let file_mode = fs::Mode::from_raw_mode(0o644); // `rw` for owner and `r` for others (rw-r--r--).

        let fd = match fs::openat(fs::CWD, file_path, oflags, file_mode) {
            Ok(fd) => fd,
            Err(e) => {
                eprintln!("[ERROR]: Failed to create/open report file: {e}");
                return Err(e);
            }
        };

        // Create JSON string and raw bytes. This can never fail for our situation and struct.
        let json_string = serde_json::to_string_pretty(self).unwrap();

        let json_bytes = json_string.as_bytes();

        // Write the bytes to the file descriptor
        // We loop because `rustix::io::write` may write fewer bytes than requested in one go
        let mut written = 0;
        while written < json_bytes.len() {
            match rustix::io::write(&fd, &json_bytes[written..]) {
                Ok(0) => {
                    eprintln!("[ERROR]: Error happened during export: Failed to write any data.");
                    // Return no space left anyways
                    return Err(Errno::NOSPC);
                },
                Ok(n) => {
                    written += n;
                },
                Err(Errno::INTR) => {
                    continue;
                },
                Err(e) => {
                    eprintln!("[ERROR]: Fatal I/O error occured while writing report: {e}");
                    return Err(e);
                }
            };
        }

        // Print success message
        eprintln!("[OXIDE_WATCH]: Report successfully exported to `output/usage_report.json`");
        Ok(())
    }
}