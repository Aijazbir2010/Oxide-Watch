# OxideWatch 🦀⚡

A lightweight, high-performance **micro-sandbox and process supervisor** written in modern Rust. 

OxideWatch executes, supervises, and collects telemetry on untrusted or target binaries directly using low-level Linux kernel primitives (such as `clone3`, `pidfd`, `getrusage`, and `prlimit`) without relying on high-level standard library abstractions like `std::process::Command`.

---

## 🚀 Why OxideWatch?

Most high-level sandbox tools rely on heavy container runtimes or standard library wrappers. OxideWatch was built from first principles as part of a low-level systems engineering curriculum to master:
*   **Raw Linux System Calls:** Directly invoking kernel interfaces using `libc` and type-safe wrappers via `rustix`.
*   **Process Lifecycle Management:** Spawning processes via `clone3` with `CLONE_PIDFD`, polling process file descriptors (`pidfd`), and cleanly reaping zombies using `waitid`.
*   **Resource Enforcement:** Enforcing hard address space memory caps (`prlimit` / `RLIMIT_AS`) and watchdog timeouts.
*   **System Telemetry:** Extracting post-mortem performance counters via `getrusage` and exporting structured JSON reports.

---

## 🛠️ Architecture & Core Modules

The codebase is engineered with strict separation of concerns into dedicated modules:

*   **`main.rs`** — Orchestrates the execution pipeline, handles top-level error bubbling, coordinates the parent-child synchronization barrier, and controls exit codes.
*   **`cli.rs`** — Robust command-line argument parser built using `clap`, providing validated bounds for timeouts (ms), memory caps (MB), and arbitrary binary arguments.
*   **`sandbox.rs`** — Encapsulates process creation (`clone3`), parent-death protection (`PR_SET_PDEATHSIG`), address space memory limitations (`prlimit`), and image replacement via `execve`.
*   **`supervisor.rs`** — Handles asynchronous monitoring via `poll` on `pidfd`, implements a precise deadline-aware watchdog timer, forces termination on timeout via `SIGKILL`, and safely reaps process zombies with `waitid`.
*   **`metrics.rs`** — Interfaces with `getrusage` to extract deep resource telemetry (CPU user/system time, peak RSS memory, page faults, and context switches) and exports it formatted via `serde`.

---

## 📋 Installation & Prerequisites

*   **OS:** Linux (Requires modern kernel supporting `clone3` and `pidfd`).
*   **Toolchain:** Rust (Stable) with Cargo.

Clone the repository and build in release mode:

```bash
git clone https://github.com/Aijazbir2010/Oxide-Watch.git
cd oxide-watch
cargo build --release
```

## 💻 Usage

OxideWatch accepts a target binary, execution constraints, and optional export flags, followed by any arguments required by the target binary.

### Syntax

```bash
cargo run --release -- --binary <PATH_TO_BINARY> [OPTIONS] -- [ARGS...]
```

### Options

- `-b, --binary <PATH>` — Path to the target binary you want to run (Required).

- `-t, --time <MS>` — Maximum execution time allowed in milliseconds (Default: `5000`, Range: `50..=60000`).

- `-m, --memory <MB>` — Maximum virtual memory limit in megabytes (Default: `256`, Range: `128..=4096`).

- `-e, --export` — Export a detailed JSON resource usage report to `output/usage_report.json`.

### Examples

1. Run a simple command with custom limits and print telemetry:

```bash
cargo run --release -- --binary /bin/ls -- -la /tmp
```

2. Run a binary and export a structured JSON report:

```bash
cargo run --release -- --binary ./target/release/my_untrusted_bin --export
```

## 📊 Sample Output

When executing a monitored process, OxideWatch streams telemetry directly to `stderr`:

```Plaintext
[OXIDE_WATCH]: Spawned process with PID: 74212

--- <PROCESS_74212_OUTPUT_START> ---
Hello from the sandboxed binary!
--- <PROCESS_74212_OUTPUT_END> ---

[SUPERVISOR]: Process 74212 exited with code: 0

--- Resource Usage ---
User CPU Time: 0.002412
System CPU Time: 0.001105
Max RSS (Memory): 3844 KB
Major Page Faults: 0
----------------------
```

## 🛡️ Low-Level Safety & Robustness Highlights

- **Interrupt Resilience (EINTR):** All blocking polling and reading loops explicitly catch and handle `EINTR` signals to prevent premature failures under heavy system load.

- **Orphan Mitigation:** Configures `PR_SET_PDEATHSIG` so that if the parent supervisor dies unexpectedly, the kernel immediately terminates the sandboxed child, avoiding runaway background processes.

## 📜 License
This project is open-source under the MIT License.