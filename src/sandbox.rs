use crate::cli::CliArgs;
use std::path::PathBuf;
use std::ffi::{CString, c_char};
use std::mem::MaybeUninit;
use std::error::Error;
use rustix::process;
use rustix::io::Errno;

pub(crate) struct Sandbox {
    pub(crate) binary: PathBuf,
    pub(crate) timeout_ms: u64,
    pub(crate) memory_cap_mb: usize,
    pub(crate) export_usage_report: bool,
    pub(crate) args_to_binary: Vec<String>
}

// Helper struct for response of `spawn_process` method
pub(crate) struct ProcessSpawnResponse {
    pub(crate) pid: libc::c_int,
    pub(crate) pidfd_raw: libc::c_int
}

impl Sandbox {
    // Create a new sandbox instance
    pub fn new(cli_args: CliArgs) -> Self {
        Self {
            binary: cli_args.binary,
            timeout_ms: cli_args.timeout,
            memory_cap_mb: cli_args.memory,
            export_usage_report: cli_args.export,
            args_to_binary: cli_args.args
        }
    }

    // Spawn the process using `clone3`
    pub fn spawn_process(&self) -> Result<ProcessSpawnResponse, Errno> {
        // `clone_args` does not have a Default implementation so we initialize it
        // with zeroes.
        let mut cl_args: libc::clone_args = unsafe {
            MaybeUninit::zeroed().assume_init()
        };

        // Configure Clone Arguments
        // Set `CLONE_PIDFD` flag so we get a `pidfd`
        cl_args.flags = libc::CLONE_PIDFD as u64;

        // Get Process File Descriptor (pidfd)
        let mut pidfd_raw: libc::c_int = -1;
        cl_args.pidfd = &mut pidfd_raw as *mut libc::c_int as u64;

        // Set the exit signal to SIGCHLD (17) so the parent is notified when child
        // terminates / exits.
        cl_args.exit_signal = libc::SIGCHLD as u64;

        // Make the raw system call
        let res = unsafe {
            // syscall number, *clone_args, size_of<clone_args>
            libc::syscall(
                libc::SYS_clone3,
                &mut cl_args as *mut libc::clone_args,
                std::mem::size_of::<libc::clone_args>()
            )
        };

        // Return Errno if `syscall` failed
        if res == -1 {
            let raw_err = unsafe { *libc::__errno_location() };
            let err = Errno::from_raw_os_error(raw_err);
            return Err(err);
        }

        Ok(ProcessSpawnResponse {
            pid: res as libc::c_int,
            pidfd_raw
        })
    }

    // Set signal to kill the process if parent thread dies.
    pub fn set_kill_signal(&self) -> Result<(), Errno> {
        // Set kill signal
        let res = unsafe {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong, 0, 0, 0)
        };

        if res == -1 {
            let raw_err = unsafe { *libc::__errno_location() };
            let err = Errno::from_raw_os_error(raw_err);
            return Err(err);
        }

        // Check if the parent already died and we've been adopted by `init`.
        // Commit suicide manually to fix the race condition (i.e. parent died
        // before the `prctl` call).
        if unsafe { libc::getppid() } == 1 {
            // Return EPIPE (Broken Pipe) to signal that parent died.
            return Err(Errno::PIPE);
        }

        Ok(())
    }

    // Set memory limits (cap) on the process.
    // Instructions: To be called in spawned process because limits are set
    // for calling process.
    pub fn set_memory_cap(&self) -> Result<(), Errno> {
        // Create a new `RLimit` instance
        let new_limit = process::Rlimit { // None represents RLIM_INFINITY (unlimited)
            current: Some((self.memory_cap_mb as u64) * 1024 * 1024),
            maximum: Some((self.memory_cap_mb as u64) * 1024 * 1024),
        };

        // Call `prlimit()` (will return old limits on success)
        // - `None` (arg 1) targets the calling process (PID 0)
        // - `Resource::As` (arg2) specifies limit for the Address Space (Virtual Memory)
        //   for the process in bytes.
        process::prlimit(None, process::Resource::As, new_limit)?;

        Ok(())
    }

    // Call `execve` with the target binary and arguments.
    pub fn execute_binary(&self) -> Result<(), Box<dyn Error>> {
        // Convert PathBuf to CString
        let binary_str = self.binary.to_str()
            .ok_or("The binary path contains invalid UTF-8 characters.")?;

        let path_c = CString::new(binary_str)
            .map_err(|_| "The binary path contains an illegal nul byte character.")?;

        // Keep the owned CStrings alive so they aren't dropped
        let mut c_strings = Vec::new();

        // `argv[0]` should have the program name by convention
        c_strings.push(path_c.clone());

        // Convert and preserve all arguments
        for arg in self.args_to_binary.iter() {
            let arg_c = CString::new(arg.as_str())
                .map_err(|_| "One of the arguments to the target binary contains an illegal nul byte character.")?;
            c_strings.push(arg_c);
        }

        // Create the arguments vector
        let mut argv: Vec<*const c_char> = c_strings.iter().map(|cs| cs.as_ptr()).collect();
        
        // Push null terminator to the vector
        argv.push(std::ptr::null());

        // Create the environment vector
        let envp: [*const c_char; 1] = [std::ptr::null()];

        // Call `execve()`
        unsafe {
            libc::execve(
                path_c.as_ptr(),
                argv.as_ptr(),
                envp.as_ptr()
            );
        }

        // This runs only if `execve()` fails
        let raw_err = unsafe { *libc::__errno_location() };
        let err = Errno::from_raw_os_error(raw_err);
        Err(Box::new(err))
    }
}