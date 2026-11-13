//! Daemon de sessões do sandmux. Veja `sandmux::daemon`.

fn main() -> std::process::ExitCode {
    sandmux::daemon::main()
}
