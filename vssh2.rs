use std::io;
use std::env;
use std::path::Path;
use nix::unistd::{fork, ForkResult};
use nix::sys::wait::waitpid;
use std::ffi::CString;
use nix::{fcntl::{OFlag, open}, sys::stat::Mode, unistd::{pipe, close, dup2, execvp}}; //, write}};

fn main() {
    loop {
        let current_dir = env::current_dir().unwrap();
        println!("{}$ ", current_dir.to_str().unwrap());

        let mut user_input = String::new();
        // following line was crafted by ChatGPT using the prompt: "I'm making a basic shell, why doesn't it read my user input correctly [code]"
        io::stdin().read_line(&mut user_input).unwrap();
        let user_input = user_input.trim();

        if user_input == "exit" {
            break;
        } else if user_input.is_empty() {
            continue;
        } else if user_input.starts_with("cd ") {
            let new_dir = &user_input[3..];
            let new_dir = Path::new(new_dir);
            // env::set_current_dir(new_dir).unwrap();
            match env::set_current_dir(new_dir) {
                Ok(_) => {},
                Err(e) => {
                    println!("Error: {}", e);
                },
            };
        } else {
            // split by '|'
            match pipe_kickoff(user_input) {
                Ok(_) => {}
                Err(e) => {eprintln!("Error: {e}")}
            }
        }
    }
}

fn pipe_kickoff(mut user_input: &str) -> anyhow::Result<()> {
    // Cstring vec for each command -- didn't work need i32
    // let mut pipes: Vec<Vec<CString>> = Vec::new();
    // heavily influenced by fork_ls_demo.rs from class

    let mut output_fd = 1;

    let background = user_input.ends_with("&");
    if background {
        user_input = &user_input[..user_input.len() -1]
    }

    if user_input.contains('>') {
        let parts: Vec<&str> = user_input.split('>').collect();
        user_input = parts[0].trim();
        let file_name = parts[1].trim();
        // output_file = Some(File::create(file_name)?);
        let flags: OFlag = [OFlag::O_CREAT, OFlag::O_WRONLY, OFlag::O_TRUNC].iter().copied().collect();
        let mode: Mode = [Mode::S_IRUSR, Mode::S_IWUSR].iter().copied().collect();
        output_fd = open(file_name, flags, mode)?;
    }

    let commands: Vec<&str> = user_input.split("|").collect();
    match unsafe { fork()? } {
        ForkResult::Parent { child, .. } => {
            if !background {
                println!("Continuing execution in parent process, new child has pid: {}", child);
                waitpid(child, None).unwrap();
                println!("Returned to parent - child is finished.");
            } else {
                println!("Running in background as pid: {}", child);
            }
        }
        ForkResult::Child => {
            // for loop of commands
            for i in (0..commands.len()).rev() {
                let get_args = externalize(commands[i]);
                eprintln!("command {i}: {:?}", get_args);
                if i == 0 {
                    if user_input.contains('<') {
                        let parts: Vec<&str> = user_input.split('<').collect();
                        user_input = parts[0].trim();
                        let file_name = parts[1].trim();
                        let input_fd = open(file_name, OFlag::O_RDONLY, Mode::empty())?;
                        dup2(input_fd, 0)?;
                    }
                    eprintln!("redirect out to {output_fd}");
                    dup2(output_fd, 1)?;
                    match execvp(&get_args[0], &get_args) {
                        Ok(_) => {
                            println!("Child process finished");
                        }
                        Err(e) => {
                            println!("Error: {}", e);
                            std::process::exit(1);
                        }
                    }
                } else {
                    // create 1 pipe here, 
                    let (read_fd, write_fd) = pipe()?; 
                    eprintln!("pipe ({}): {read_fd} {write_fd}", commands[i]);
                    match unsafe {fork()?} {
                        ForkResult::Parent { child:_, .. } => {
                            eprintln!("redirect in to {read_fd}, out to {output_fd}");
                            close(write_fd)?;
                            dup2(read_fd, 0)?;
                            dup2(output_fd, 1)?;
                            println!("executing");
                            // Execute
                            match execvp(&get_args[0], &get_args) {
                                Ok(_) => {
                                    println!("Child process finished");
                                }
                                Err(e) => {
                                    println!("Error: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        ForkResult::Child => {
                            output_fd = write_fd;
                            close(read_fd)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn externalize(command: &str) -> Vec<CString> {
    // add splitting of | ??
    let mut command = command.to_string();
    if command.ends_with("&") {
        command.pop();
    }
    command.split_whitespace()
        .map(|s| CString::new(s).unwrap())
        .collect()
}
