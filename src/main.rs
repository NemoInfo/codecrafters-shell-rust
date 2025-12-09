use std::{
  io::{self, Write},
  os::unix::fs::PermissionsExt,
  path::PathBuf,
};

mod split;
use split::*;

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
            return Some(path);
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

enum CommandKind {
  Builtin(Builtin),
  Program(PathBuf),
  NotFound(String),
}

impl CommandKind {
  fn parse(command: &str, paths: &Vec<PathBuf>) -> Self {
    let command = command.trim();
    if let Some(builtin) = Builtin::try_parse(command) {
      CommandKind::Builtin(builtin)
    } else if let Some(program) = search(paths, command) {
      CommandKind::Program(program)
    } else {
      CommandKind::NotFound(command.to_owned())
    }
  }
}

struct Command {
  kind: CommandKind,
  args: Vec<String>,
}

enum ControlFlow {
  Repl,
  Exit,
}

impl Command {
  fn from_split(command: String, args: Vec<String>, paths: &Vec<PathBuf>) -> Self {
    Self { kind: CommandKind::parse(&command, paths), args }
  }

  fn run(&self, paths: &Vec<PathBuf>, control_flow: &mut ControlFlow, mut stdout: Box<dyn Write>) {
    use Builtin::*;
    match &self.kind {
      CommandKind::Builtin(Exit) => *control_flow = ControlFlow::Exit,
      CommandKind::Builtin(Type) => {
        for arg in &self.args {
          match CommandKind::parse(arg, paths) {
            CommandKind::Builtin(builtin) => {
              writeln!(stdout, "{} is a shell builtin", builtin.to_string()).unwrap();
              io::stdout().flush().unwrap();
            }
            CommandKind::Program(path) => {
              writeln!(stdout, "{}", path.display()).unwrap();
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
        writeln!(stdout, "{}", self.args.join(" ")).unwrap();
        io::stdout().flush().unwrap();
      }
      CommandKind::Builtin(Pwd) => {
        let path = std::env::current_dir().unwrap();
        writeln!(stdout, "{}", path.display()).unwrap();
        io::stdout().flush().unwrap();
      }
      CommandKind::Builtin(Cd) => {
        let home = std::env::var("HOME").unwrap();
        let path: PathBuf = self.args.first().unwrap_or(&"~".to_owned()).replace("~", &home).into();
        std::env::set_current_dir(&path).unwrap_or_else(|_| {
          eprintln!("cd: {}: No such file or directory", path.display());
          io::stderr().flush().unwrap();
        });
      }
      CommandKind::Program(path) => {
        let output = std::process::Command::new(path.file_name().unwrap())
          .args(&self.args)
          .output()
          .expect("Running command failed");
        if output.status.success() {
          write!(stdout, "{}", str::from_utf8(&output.stdout).unwrap()).unwrap();
          io::stdout().flush().unwrap();
        } else {
          eprint!("{}", str::from_utf8(&output.stderr).unwrap());
          io::stderr().flush().unwrap();
        }
      }
      CommandKind::NotFound(name) => {
        eprintln!("{name}: command not found")
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

    if let Ok(args) = split(&input) {
      let mut args = args.into_iter();
      let Some(command) = args.next() else { continue };
      enum State {
        RedirectStdout,
        Arg,
      }
      use State::*;
      let mut state = Arg;
      let mut actual_args = vec![];
      let mut stdout: Box<dyn Write> = Box::new(std::io::stdout());
      for arg in args {
        state = match state {
          Arg => match arg.as_str() {
            ">" | "1>" => RedirectStdout,
            _ => {
              actual_args.push(arg);
              Arg
            }
          },
          RedirectStdout => {
            stdout = Box::new(std::fs::File::create(arg).unwrap());
            Arg
          }
        }
      }
      Command::from_split(command, actual_args, &paths).run(&paths, &mut control_flow, stdout);
    } else {
      eprintln!("Syntax error");
      io::stderr().flush().unwrap();
    }
  }
}
