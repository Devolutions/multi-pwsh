fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();

    match pwsh_host::run_pwsh_command_line(args) {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(error) => {
            eprintln!("pwsh-host-cli: {}", error);
            std::process::exit(1);
        }
    }
}
