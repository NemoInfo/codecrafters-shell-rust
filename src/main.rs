use std::{
  io::{self, Write},
  os::unix::fs::PermissionsExt,
  path::PathBuf,
};

fn search(paths: &Vec<PathBuf>, command: &str) -> Option<String> {
  for path in paths {
    if path.is_file() {
      let name = path.file_name()?.to_str()?;
      let is_exec = path.metadata().ok()?.permissions().mode() & 0o111 != 0;
      if name == command && is_exec {
        return Some(path.display().to_string());
      }
    } else if path.is_dir() {
      let entries = std::fs::read_dir(path).ok()?;
      for entry in entries {
        let path = entry.ok()?.path();
        if path.is_file() {
          let name = path.file_name()?.to_str()?;
          let is_exec = path.metadata().ok()?.permissions().mode() & 0o111 != 0;
          if name == command && is_exec {
            return Some(path.display().to_string());
          }
        }
      }
    }
  }
  None
}

fn main() {
  let path = std::env::var("PATH").unwrap();
  let paths: Vec<_> = std::env::split_paths(&path).collect();

  loop {
    print!("$ ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).expect("Expected command");
    let mut args = input.trim().split(" ").collect::<Vec<_>>().into_iter();
    let command = args.next().unwrap();

    match command.trim() {
      "exit" => break,
      "echo" => {
        let rest = args.collect::<Vec<_>>().join(" ");
        println!("{rest}")
      }
      "type" => {
        let command = args.next().expect("Expected argument");
        match command {
          "echo" | "exit" | "type" => {
            println!("{command} is a shell builtin");
            io::stdout().flush().unwrap();
          }
          _ => match search(&paths, command) {
            Some(path) => {
              println!("{command} is {path}");
              io::stdout().flush().unwrap();
            }
            None => {
              println!("{command}: not found");
              io::stdout().flush().unwrap();
            }
          },
        }
      }
      command => match search(&paths, command) {
        Some(_program) => {
          let output = std::process::Command::new(command)
            .args(args)
            .output()
            .expect("Running command failed");
          if output.status.success() {
            print!("{}", str::from_utf8(&output.stdout).unwrap());
          } else {
            print!("{}", str::from_utf8(&output.stderr).unwrap());
          }
          io::stdout().flush().unwrap();
        }
        None => {
          println!("{command}: command not found");
          io::stdout().flush().unwrap();
        }
      },
    }
  }
}
