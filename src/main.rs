const PROGRAM_VERSION: &str = "0.22.1";

fn main() -> std::process::ExitCode {
    skout::cli::run(PROGRAM_VERSION)
}
