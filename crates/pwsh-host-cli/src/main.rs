mod named_pipe_command;

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let args = match named_pipe_command::preprocess_named_pipe_command_args(args) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("pwsh-host: {}", error);
            std::process::exit(1);
        }
    };

    match pwsh_host::run_pwsh_command_line(args) {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(error) => {
            eprintln!("pwsh-host: {}", error);
            std::process::exit(1);
        }
    }
}
