// Declare mods
mod cli;
mod sandbox;
mod supervisor;
mod metrics;

// Imports from modules
use crate::cli::CliArgs;
use crate::sandbox::Sandbox;
use crate::supervisor::Supervisor;
use crate::metrics::Report;

// Imports from standard library
use std::io::Write;

// Imports from external crates
use clap::Parser;
use rustix::io::Errno;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI arguments
    let cli_args = CliArgs::parse();
    
    // Create a new sandbox
    let sandbox = Sandbox::new(cli_args);

    // Create a synchronization pipe before `clone3`
    let (pipe_read, pipe_write) = rustix::pipe::pipe().inspect_err(|e| {
        eprintln!("[ERROR]: Failed to create the synchronization pipeline: {e}");
    })?;

    // Spawn the process
    let response = sandbox.spawn_process().inspect_err(|e| {
        eprintln!("[ERROR]: Failed to spawn the process: {e}");
    })?;

    // Handle child and parent process
    match response.pid {
        // CHILD PROCESS
        0 => {
            // Drop the write end so the child doesn't keep it open.
            drop(pipe_write);

            // Block here. Wait for the parent to drop its write end (triggers EOF).
            let mut buf = [0u8; 1];
            loop {
                match rustix::io::read(&pipe_read, &mut buf) {
                    Ok(_) => { break; }
                    Err(e) => {
                        // Continue the loop if the call was interrupted.
                        if e == Errno::INTR {
                            continue;
                        }
                        eprintln!("[ERROR]: Failed to read from the pipeline: {e}");
                        // It's important to exit on error in child so that it can't escape
                        // to the main logic.
                        unsafe { libc::_exit(1) }
                    }
                };
            }

            // Clean up the read end before changing the process image.
            drop(pipe_read);

            // Set kill signal (in case the parent dies) and handle the error manually and exit.
            if let Err(e) = sandbox.set_kill_signal() {
                eprintln!("[ERROR]: Failed to set kill signal: {e}");
                unsafe { libc::_exit(1) }
            };

            // Set memory cap and handle the error manually and exit.
            if let Err(e) = sandbox.set_memory_cap() {
                eprintln!("[ERROR]: Failed to set memory cap: {e}");
                if e == Errno::PERM {
                    eprintln!(
                        "[HINT]: You cannot request a memory cap higher than the current environment allows. \
                        If you are running this sandbox recursively, ensure the nested memory limit is lower than \
                        or equal to the parent limit."
                    );
                }
                unsafe { libc::_exit(1) }
            };

            // Execute the target binary and handle the error manually and exit.
            if let Err(e) = sandbox.execute_binary() {
                eprintln!("[ERROR]: Failed to execute target binary: {e}");
                unsafe { libc::_exit(1) }
            };
        }

        // PARENT PROCESS
        child_pid => {
            // Drop the read end so the parent doesn't keep it open.
            drop(pipe_read);

            // Print Lines (it will print first because child is blocked on the read operation).
            eprintln!("[OXIDE_WATCH]: Spawned process with PID: {child_pid}");
            eprintln!();
            eprintln!("--- <PROCESS_{child_pid}_OUTPUT_START> ---");
            std::io::stderr().flush()?;

            // Unblock the child by closing / dropping the write end.
            drop(pipe_write);

            // Create a new Supervisor instance.
            let supervisor = Supervisor::new(response.pidfd_raw, child_pid, sandbox.timeout_ms);

            // Set watchdog timer (timeout) on the child process and wait for the process to
            // complete or the timeout to expire.
            supervisor.supervise()?;

            // Clean up the zombie state of the child process
            supervisor.cleanup_zombie()?;

            // Print the Usage Report (and export if required)
            let report = Report::new()?;
            report.print_resource_usage();

            if sandbox.export_usage_report {
                report.export_resource_usage()?;
            }
        }
    }

    Ok(())
}