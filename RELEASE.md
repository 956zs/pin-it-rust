# Release Notes

## Version 0.1.0

**Release Date:** 2025-08-22

This is the initial stable release of the High-Performance Discord Pin Bot. This version focuses on core functionality, stability, and significant performance improvements over traditional script-based bots.

### ✨ New Features & Highlights

*   **Voting-Based Pinning:** Messages are pinned only after reaching a configurable number of ✅ votes.
*   **Instant Pinning:** Set `CONFIRM_CAP=0` to have the bot pin messages instantly upon being mentioned.
*   **Robust & Concurrent:** Built with Rust's Serenity framework, ensuring thread-safety and memory-safety.
*   **High Performance:** Utilizes asynchronous operations (`tokio`) and concurrent data structures (`dashmap`) for minimal latency.
*   **Automatic Session Cleanup:** Old voting sessions are automatically cleaned up to prevent memory leaks.
*   **Rate Limiting:** Basic protection against API abuse is built-in.
*   **Comprehensive Logging:** Integrated `tracing` for detailed logging and easier debugging.

### 🐛 Bug Fixes

*   **Concurrency Fix:** Resolved a critical compilation error by changing `AtomicU32` to `Arc<AtomicU32>` in the `VotingSession` struct. This ensures the struct is `Clone`-able while maintaining thread-safe atomic operations for vote counting.

### 🚀 Performance Improvements

*   **Faster Compilation:** Added a Cargo configuration file (`.cargo/config.toml`) to use the `lld` linker, which can significantly speed up project compilation times, especially the linking phase. See `README.md` for setup instructions.

### 📦 How to Use This Release

1.  **Prerequisites:** Ensure you have Rust and the necessary linker tools (`clang`, `lld`) installed on your system.
2.  **Configuration:** Copy `.env.example` to `.env` and fill in your Discord bot `TOKEN` and other settings.
3.  **Build the Release Binary:**
    ```bash
    cargo build --release
    ```
4.  **Run the Bot:**
    From the project's root directory, execute the compiled binary:
    ```bash
    ./target/release/discord-pin-bot
    ```
    For long-term deployment, consider using the Docker or Systemd examples provided in the `README.md`.

### Assets (Pre-compiled Binaries)

For user convenience, we provide pre-compiled binaries for major operating systems. Download the appropriate file for your system, extract it, and run the executable.

| File                                     | Operating System | Architecture | SHA256 Checksum                 |
| ---------------------------------------- | ---------------- | ------------ | ------------------------------- |
| `discord-pin-bot-v0.1.0-x86_64-linux.tar.gz` | Linux            | x86_64       | `(generate after packaging)`    |
| `discord-pin-bot-v0.1.0-x86_64-windows.zip`| Windows          | x86_64       | `(generate after packaging)`    |

**Note on Security:** The source code is always available for you to audit and compile yourself. The checksums are provided to verify that the downloaded files have not been tampered with.

### How to Verify Checksums

To get the checksum for your packaged files (to fill in the table above), or for users to verify their downloads, use the following commands:

*   **On Linux:**
    ```bash
    sha256sum ./target/discord-pin-bot-linux.tar.gz
    ```
*   **On Windows (PowerShell):**
    ```powershell
    Get-FileHash -Algorithm SHA256 .\target\discord-pin-bot-windows.zip
    ```