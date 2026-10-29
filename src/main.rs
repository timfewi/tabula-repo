#![forbid(unsafe_code)]
fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(tabula::cli::main() as u8)
}
