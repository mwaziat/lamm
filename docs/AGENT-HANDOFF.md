# Agent handoff — read first

## Mission
Develop LAMM as one desktop Linux application that coordinates OS-level resource management and cooperative app-level lifecycle optimization. User requested focusing on implementation foundation now; functional/policy detail will be agreed later in conversation. **Do not invent sleep, kill or reclaim policies.**

## Current source
This ZIP is the authoritative initial skeleton; no previous code exists. Core, daemon, CLI, UI and scripts are included. No root privileges or service-level resource management currently used.

## First tasks
1. Install Rust stable, GTK4/libadwaita headers on Ubuntu GNOME 50.1/Wayland.
2. Run `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets`.
3. Fix compile/API issues; no local Rust environment was available during generation.
4. Verify daemon and CLI; verify GTK UI including service disabled case.
5. Document measurements and corrections before adding optimization.

## Constraints
- Keep daemon light and able to run without UI.
- Preserve no-destructive-action safety invariant.
- No unsafe browser process injection, `/proc/pid/mem`, or undocumented GNOME APIs.
- Avoid privileged modifications until explicit design approval.
- Do not claim RSS equals unique app RAM usage.
- Test split-screen and active-tab details only after legitimate integration exists.

## Known issues
- UI currently does blocking socket reads in main thread; improve via worker/async.
- Protocol is one newline-delimited JSON snapshot per connection; no versioning yet.
- No authentication beyond Unix socket owner permission; same-user clients can connect.
- Daemon has no cancellation/shutdown cleanup hook (stale socket cleaned on restart).
- Daemon reads `/proc` per request, potentially costly under rapid polling.
- Desktop entry requires PATH resolution for `lamm-ui`; consider absolute Exec or system integration.
- Docs use GTK4 0.10 / libadwaita 0.8 bindings; validate compat on real toolchain.
