use std::{
  io::{self, Write},
  os::unix::fs::PermissionsExt,
  path::PathBuf,
};

fn search(paths: &Vec<PathBuf>, command: &str) -> Option<PathBuf> {
  for path in paths {
    if path.is_file() {
      let name = path.file_name()?.to_str()?;
      let is_exec = path.metadata().ok()?.permissions().mode() & 0o111 != 0;
      if name == command && is_exec {
        return Some(path.clone());
      }
    } else if path.is_dir() {
      let entries = std::fs::read_dir(path).ok()?;
      for entry in entries {
        let path = entry.ok()?.path();
        if path.is_file() {
          let name = path.file_name()?.to_str()?;
          let is_exec = path.metadata().ok()?.permissions().mode() & 0o111 != 0;
          if name == command && is_exec {
            return Some(path.into());
          }
        }
      }
    }
  }
  None
}

enum Builtin {
  Exit,
  Type,
  Echo,
  Pwd,
  Cd,
}

impl Builtin {
  fn try_parse(command: &str) -> Option<Self> {
    use Builtin::*;
    let command = command.trim();
    match command {
      "exit" => Some(Exit),
      "type" => Some(Type),
      "echo" => Some(Echo),
      "pwd" => Some(Pwd),
      "cd" => Some(Cd),
      _ => None,
    }
  }

  fn to_string(&self) -> &'static str {
    use Builtin::*;
    match self {
      Exit => "exit",
      Type => "type",
      Echo => "echo",
      Pwd => "pwd",
      Cd => "cd",
    }
  }
}

enum CommandKind<'a> {
  Builtin(Builtin),
  Program(PathBuf),
  NotFound(&'a str),
}

impl<'a> CommandKind<'a> {
  fn parse(command: &'a str, paths: &Vec<PathBuf>) -> Self {
    let command = command.trim();
    if let Some(builtin) = Builtin::try_parse(command) {
      CommandKind::Builtin(builtin)
    } else if let Some(program) = search(paths, command) {
      CommandKind::Program(program)
    } else {
      CommandKind::NotFound(command)
    }
  }
}

struct Command<'a> {
  kind: CommandKind<'a>,
  args: Vec<&'a str>,
}

enum ControlFlow {
  Repl,
  Exit,
}

impl<'a> Command<'a> {
  fn from_iter(args: impl IntoIterator<Item = &'a str>, paths: &Vec<PathBuf>) -> Self {
    let mut args = args.into_iter();
    let command = args.next().unwrap();
    Self { kind: CommandKind::parse(command, paths), args: args.collect() }
  }

  fn run(&self, paths: &Vec<PathBuf>, control_flow: &mut ControlFlow) {
    use Builtin::*;
    match &self.kind {
      CommandKind::Builtin(Exit) => *control_flow = ControlFlow::Exit,
      CommandKind::Builtin(Type) => {
        for &arg in &self.args {
          match CommandKind::parse(arg, paths) {
            CommandKind::Builtin(builtin) => {
              println!("{} is a shell builtin", builtin.to_string());
              io::stdout().flush().unwrap();
            }
            CommandKind::Program(path) => {
              println!("{}", path.display());
              io::stdout().flush().unwrap();
            }
            CommandKind::NotFound(name) => {
              eprintln!("{name}: not found");
              io::stderr().flush().unwrap();
            }
          }
        }
      }
      CommandKind::Builtin(Echo) => {
        println!("{}", self.args.join(" "));
        io::stdout().flush().unwrap();
      }
      CommandKind::Builtin(Pwd) => {
        let path = std::env::current_dir().unwrap();
        println!("{}", path.display());
        io::stdout().flush().unwrap();
      }
      CommandKind::Builtin(Cd) => {
        let path: PathBuf = self.args[0].into();
        std::env::set_current_dir(&path).unwrap_or_else(|_| {
          eprintln!("cd {}: No such file or directory", path.display());
          io::stderr().flush().unwrap();
        });
      }
      CommandKind::Program(path) => {
        let output = std::process::Command::new(path.file_name().unwrap())
          .args(&self.args)
          .output()
          .expect("Running command failed");
        if output.status.success() {
          print!("{}", str::from_utf8(&output.stdout).unwrap());
          io::stdout().flush().unwrap();
        } else {
          eprint!("{}", str::from_utf8(&output.stderr).unwrap());
          io::stderr().flush().unwrap();
        }
      }
      CommandKind::NotFound(name) => {
        println!("{name}: command not found")
      }
    }
  }
}

fn main() {
  let path = std::env::var("PATH").unwrap();
  let paths: Vec<_> = std::env::split_paths(&path).collect();
  let mut control_flow = ControlFlow::Repl;

  while let ControlFlow::Repl = &control_flow {
    print!("$ ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).expect("Expected command");
    let args = input.trim().split(" ").collect::<Vec<_>>().into_iter();

    let command = Command::from_iter(args, &paths);
    command.run(&paths, &mut control_flow);
  }
}
