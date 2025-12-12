use std::fmt::Display;
use std::path::PathBuf;
use std::{io::Write, str::FromStr};

use anyhow::{Context, anyhow};

use crate::{CommandErr, CommandIn, CommandKind, CommandOut, ControlFlow};

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

pub struct State {
  pub control_flow: ControlFlow,
  pub history: Vec<String>,
}

impl State {
  pub fn new() -> Self {
    Self {
      control_flow: ControlFlow::Repl,
      history: vec![],
    }
  }
}

impl Builtin {
  pub const TO_STRING: [&'static str; 6] = ["exit", "type", "echo", "pwd", "cd", "history"];

  pub fn run(
    &self,
    state: &mut State,
    stdout: CommandOut,
    mut stderr: CommandErr,
    stdin: Option<CommandIn>,
    paths: &Vec<PathBuf>,
    args: Vec<String>,
  ) -> anyhow::Result<()> {
    if let Err(e) = self._run(state, stdout, &mut stderr, stdin, paths, args) {
      writeln!(stderr, "{self}: {e}")?;
      return Err(e);
    };
    Ok(())
  }

  fn _run(
    &self,
    state: &mut State,
    mut stdout: CommandOut,
    stderr: &mut CommandErr,
    _stdin: Option<CommandIn>,
    paths: &Vec<PathBuf>,
    args: Vec<String>,
  ) -> anyhow::Result<()> {
    match self {
      Builtin::Exit => state.control_flow = ControlFlow::Exit,
      Builtin::Type => {
        for arg in args {
          match CommandKind::parse(&arg, paths) {
            CommandKind::Builtin(name) => writeln!(stdout, "{name} is a shell builtin")?,
            CommandKind::Program(path) => writeln!(stdout, "{}", path.display())?,
            CommandKind::NotFound(name) => writeln!(stderr, "{name}: not found")?,
          }
        }
      }
      Builtin::Echo => writeln!(stdout, "{}", args.join(" "))?,
      Builtin::Pwd => {
        let path = std::env::current_dir()?;
        writeln!(stdout, "{}", path.display())?;
      }
      Builtin::Cd => {
        let home = std::env::var("HOME")?;
        let path: PathBuf = args.first().unwrap_or(&"~".to_owned()).replace("~", &home).into();
        std::env::set_current_dir(&path)
          .context(format!("{}: No such file or directory", path.display()))?;
      }
      Builtin::History => {
        let [mut r, mut w, mut a] = [None, None, None];
        let mut n = state.history.len();

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
          match arg.as_str() {
            "-r" => r = Some(args.next().ok_or(anyhow!("expected <history_file>"))?),
            "-w" => w = Some(args.next().ok_or(anyhow!("expected <history_file>"))?),
            "-a" => a = Some(args.next().ok_or(anyhow!("expected <history_file>"))?),
            val => {
              let val = val.parse().context(format!("could not parse number `{val}`"))?;
              args.next().map_or(Ok(()), |e| Err(anyhow!("unexpected argument `{e}`")))?;
              n = val;
            }
          }
        }

        if w.is_some() && a.is_some() {
          return Err(anyhow!("options -w and -a are mutually exclusive"));
        }

        if let Some(history_file_path) = r {
          state.history.append(
            &mut std::fs::read_to_string(&history_file_path)
              .map(|x| x.lines().map(str::to_owned).collect::<Vec<_>>())
              .context(format!("unable to read file `{history_file_path}`"))?,
          );
          return Ok(());
        }

        let shown = state
          .history
          .iter()
          .enumerate()
          .rev()
          .take(n)
          .rev()
          .map(|(i, s)| format!("{:>5}  {s}", i + 1))
          .collect::<Vec<_>>()
          .join("\n");

        if let Some(history_file_path) = w {
          std::fs::File::create(&history_file_path)
            .context(format!("unable to open file `{history_file_path}`"))?
            .write_all(shown.as_bytes())
            .context(format!("unable to write to file `{history_file_path}`"))?;
        }

        writeln!(stdout, "{shown}")?;
      }
    }
    Ok(())
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
