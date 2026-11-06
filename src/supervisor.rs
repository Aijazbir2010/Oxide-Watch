use std::os::fd::{FromRawFd, OwnedFd, AsFd};
use std::time::{Instant, Duration};
use rustix::{event, time, process};
use rustix::io::Errno;

pub(crate) struct Supervisor {
    pidfd: OwnedFd,
    pid: libc::c_int,
    timeout_ms: u64,
}

impl Supervisor {
    // Creates a new supervisor instance
    pub fn new(pidfd_raw: libc::c_int, pid: libc::c_int, timeout_ms: u64) -> Self {
        let pidfd = unsafe { OwnedFd::from_raw_fd(pidfd_raw) };
        Self {
            pidfd,
            pid,
            timeout_ms
        }
    }

    // Sets a watchdog timer (timeout) for the target process and kills it if timeout expires.
    pub fn supervise(&self) -> Result<(), Errno> {
        // Prepare to poll
        let mut poll_fds = [event::PollFd::new(&self.pidfd, event::PollFlags::IN)];
        
        // Calculate the absolute deadline before entering the loop.
        let timeout_duration = Duration::from_millis(self.timeout_ms);
        let deadline = Instant::now() + timeout_duration;
        
        // Poll blocks until the process exits or the timeout hits.
        loop {
            // Figure out how much time is left right now.
            let now = Instant::now();
            if now >= deadline { 
                // Timeout already hit while handling an interrupt or context switching.
                break;
            }
            let time_left = deadline - now;

            let timeout = time::Timespec {
                tv_sec: time_left.as_secs() as _, // Convert whole seconds
                tv_nsec: time_left.subsec_nanos() as _ // Convert remainder to nanoseconds
            };

            match event::poll(&mut poll_fds, Some(&timeout)) {
                Ok(0) => {
                    // Timeout hit (process is still running).
                    break;
                }
                Ok(_) => {
                    // `poll_fds[0]` triggered, meaning the child exited before the timeout.
                    return Ok(());
                }
                Err(Errno::INTR) => {
                    // A signal interrupted `poll`. Restart the loop.
                    continue;
                }
                Err(e) => {
                    eprintln!("[ERROR]: Process monitoring loop encountered a fatal error: {e}");
                    return Err(e);
                }
            }
        }

        // Timeout expired. Send SIGKILL to the process.
        eprintln!("[TIMEOUT]: Timeout expired! Killing process {}...", self.pid);
        if let Err(e) = process::pidfd_send_signal(&self.pidfd, process::Signal::KILL) {
            // It's not fatal if the process is already dead and reaped. (This error is not likely
            // to happen in our case most of the time).
            if e != Errno::SRCH {
                eprintln!("[ERROR]: Failed to terminate the process: {e}");
                return Err(e);
            }
        }

        Ok(())
    }

    // Cleans up the zombie state of the process.
    pub fn cleanup_zombie(&self) -> Result<(), Errno> {
        // Clean up the zombie process state safely via `waitid()`
        loop {
            match process::waitid(
                process::WaitId::PidFd(self.pidfd.as_fd()),
                process::WaitIdOptions::EXITED
            ) {
                Ok(Some(status)) => {
                    // Print for showing end of process's output
                    eprintln!("--- <PROCESS_{}_OUTPUT_END> ---", self.pid);
                    eprintln!();
    
                    // Check if process exited on its own or was killed
                    if status.exited() { // process exited normally
                        eprintln!(
                            "[SUPERVISOR]: Process {} exited with code: {}",
                            self.pid,
                            status.exit_status().unwrap()
                        );
                    } else if status.killed() { // process terminated through signal
                        let signal_code = status.terminating_signal().unwrap();
                        let signal = process::Signal::from_named_raw(signal_code);
    
                        // If the memory limit is hit, the process will likely abort (SIGABRT) or crash (SIGSEGV)
                        if signal == Some(process::Signal::ABORT) {
                            eprintln!("[SUPERVISOR]: Process {} Aborted. LIKELY panicked due to an allocation failure.", self.pid);
                        } else if signal == Some(process::Signal::SEGV) {
                            eprintln!("[SUPERVISOR]: Process {} hit a Segmentation Fault. LIKELY breached its memory limit.", self.pid);
                        } else {
                            eprintln!("[SUPERVISOR]: Process {} was killed by signal: {}", self.pid, signal_code);
                        }
                    }
    
                    return Ok(());
                },
                // This branch will NEVER run because we are not using `NOHANG` in `WaitIdOptions`
                Ok(None) => {
                    eprintln!("[SUPERVISOR]: Process {} is still running.", self.pid);
                    continue;
                },
                Err(Errno::INTR) => {
                    // A signal interrupted `waitid`. Restart the loop.
                    continue;
                }
                Err(Errno::CHILD) => {
                    // The process was already reaped or doesn't exist (not likely).
                    // We treat this as success since the zombie is gone.
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("[ERROR]: Failed to reap the process zombie state: {e}");
                    return Err(e);
                }
            }
        }
    }
}