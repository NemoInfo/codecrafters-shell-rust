use std::fmt::Display;
use std::path::PathBuf;
use std::{io::Write, str::FromStr};

use crate::{CommandErr, CommandIn, CommandKind, CommandOut, ControlFlow};

pub const HISTORY_FILE_NAME: &str = ".history";

#[repr(usize)]
#[derive(Debug, Clone, Copy)]
pub enum Builtin {
  Exit,
  Type,
  Echo,
  Pwd,
  Cd,
  History,
}

impl Builtin {
  pub const TO_STRING: [&'static str; 6] = ["exit", "type", "echo", "pwd", "cd", "history"];

  pub fn run(
    &self,
    control_flow: &mut ControlFlow,
    mut stdout: CommandOut,
    mut stderr: CommandErr,
    _stdin: Option<CommandIn>,
    paths: &Vec<PathBuf>,
    args: &Vec<String>,
  ) {
    match self {
      Builtin::Exit => *control_flow = ControlFlow::Exit,
      Builtin::Type => {
        for arg in args {
          match CommandKind::parse(arg, paths) {
            CommandKind::Builtin(name) => writeln!(stdout, "{name} is a shell builtin").unwrap(),
            CommandKind::Program(path) => writeln!(stdout, "{}", path.display()).unwrap(),
            CommandKind::NotFound(name) => writeln!(stderr, "{name}: not found").unwrap(),
          }
        }
      }
      Builtin::Echo => writeln!(stdout, "{}", args.join(" ")).unwrap(),
      Builtin::Pwd => {
        let path = std::env::current_dir().unwrap();
        writeln!(stdout, "{}", path.display()).unwrap();
      }
      Builtin::Cd => {
        let home = std::env::var("HOME").unwrap();
        let path: PathBuf = args.first().unwrap_or(&"~".to_owned()).replace("~", &home).into();
        std::env::set_current_dir(&path).unwrap_or_else(|_| {
          writeln!(stderr, "cd: {}: No such file or directory", path.display()).unwrap();
        });
      }
      Builtin::History => {
        let Ok(history) = std::fs::read_to_string(HISTORY_FILE_NAME) else {
          writeln!(stderr, "history: could not read history file `{HISTORY_FILE_NAME}`").unwrap();
          return;
        };

        let lines = history.lines().collect::<Vec<_>>();
        let n = match args.first().map(|s| s.parse::<usize>()) {
          Some(Ok(n)) => n,
          Some(Err(_)) => {
            writeln!(stderr, "history: argument expected type usize found `{}`", args[0])
              .unwrap();
            return;
          }
          None => lines.len(),
        };
        let shown = lines.iter().rev().take(n).rev().cloned().collect::<Vec<_>>().join("\n");

        writeln!(stdout, "{shown}").unwrap();
      }
    }
  }
}

impl FromStr for Builtin {
  type Err = ();
  fn from_str(command: &str) -> Result<Self, Self::Err> {
    use Builtin::*;
    let command = command.trim();
    match command {
      "exit" => Ok(Exit),
      "type" => Ok(Type),
      "echo" => Ok(Echo),
      "pwd" => Ok(Pwd),
      "cd" => Ok(Cd),
      "history" => Ok(History),
      _ => Err(()),
    }
  }
}

impl Display for Builtin {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(Self::TO_STRING[*self as usize])
  }
}
