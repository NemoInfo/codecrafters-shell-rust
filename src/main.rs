use std::{
  io::{self, Read, Write},
  os::{fd::AsRawFd, unix::fs::PermissionsExt},
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

#[repr(usize)]
#[derive(Clone, Copy)]
enum Builtin {
  Exit,
  Type,
  Echo,
  Pwd,
  Cd,
}

impl Builtin {
  const TO_STRING: [&'static str; 5] = ["exit", "type", "echo", "pwd", "cd"];

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

  fn to_string(self) -> &'static str {
    Self::TO_STRING[self as usize]
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

  fn run(
    &self,
    paths: &Vec<PathBuf>,
    control_flow: &mut ControlFlow,
    mut stdout: Box<dyn Write>,
    mut stderr: Box<dyn Write>,
  ) {
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
              writeln!(stderr, "{name}: not found").unwrap();
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
          writeln!(stderr, "cd: {}: No such file or directory", path.display()).unwrap();
          io::stderr().flush().unwrap();
        });
      }
      CommandKind::Program(path) => {
        let output = std::process::Command::new(path.file_name().unwrap())
          .args(&self.args)
          .output()
          .expect("Running command failed");

        write!(stdout, "{}", str::from_utf8(&output.stdout).unwrap()).unwrap();
        stdout.flush().unwrap();
        write!(stderr, "{}", str::from_utf8(&output.stderr).unwrap()).unwrap();
        stderr.flush().unwrap();
      }
      CommandKind::NotFound(name) => {
        writeln!(stderr, "{name}: command not found").unwrap();
        stderr.flush().unwrap();
      }
    }
  }
}

#[derive(Debug)]
enum Key {
  Char(char),
  Backspace,
  Tab,
  Newline,
  Delete,
  LeftArrow,
  RightArrow,
  UpArrow,
  DownArrow,
  CtrlL,
  CtrlD,
}

fn main() {
  let path = std::env::var("PATH").unwrap();
  let paths: Vec<_> = std::env::split_paths(&path).collect();
  let mut control_flow = ControlFlow::Repl;

  let fd = io::stdin().as_raw_fd();
  let mut termios = unsafe {
    let mut t = std::mem::zeroed();
    libc::tcgetattr(fd, &mut t);
    t
  };

  termios.c_lflag &= !(libc::ECHO | libc::ICANON);

  unsafe {
    libc::tcsetattr(fd, libc::TCSANOW, &termios);
  }

  while let ControlFlow::Repl = &control_flow {
    print!("$ ");
    io::stdout().flush().unwrap();

    let mut bytes = [0u8; 4];
    let mut input = Vec::new();
    let mut cursor_position: usize = 0;

    loop {
      let bytes_read = io::stdin().read(&mut bytes).unwrap();
      use Key::*;
      let key = match bytes[0] {
        0x08 | 0x7F => Backspace,
        0x0C => CtrlL,
        0x04 => CtrlD,
        0x1B => {
          if bytes_read >= 3 && bytes[1] == b'[' {
            match bytes[2] {
              b'A' => UpArrow,
              b'B' => DownArrow,
              b'C' => RightArrow,
              b'D' => LeftArrow,
              b'3' => Delete,
              _ => panic!("{}", bytes[2]),
            }
          } else {
            panic!();
          }
        }
        b'\t' => Tab,
        b'\n' | b'\r' => Newline,
        ch => Char(ch as char),
      };

      match key {
        Char(ch) => {
          print!("\x1B[4h");
          print!("{}", ch);
          print!("\x1B[4l");
          std::io::stdout().flush().unwrap();
          input.insert(cursor_position, ch);
          cursor_position += 1;
        }
        RightArrow => {
          cursor_position = (cursor_position + 1).min(input.len());
          print!("\x1B[C");
          std::io::stdout().flush().unwrap();
        }
        LeftArrow => {
          cursor_position = cursor_position.saturating_sub(1);
          print!("\x1B[D");
          std::io::stdout().flush().unwrap();
        }
        Backspace => {
          if 0 < cursor_position && cursor_position <= input.len() {
            input.remove(cursor_position - 1);
            cursor_position -= 1;
            print!("\x08\x1B[1P");
            std::io::stdout().flush().unwrap();
          }
        }
        Delete => {
          if cursor_position < input.len() {
            input.remove(cursor_position);
            print!("\x1B[1P");
            std::io::stdout().flush().unwrap();
          }
        }
        Newline => {
          println!();
          std::io::stdout().flush().unwrap();
          break;
        }
        Tab => {
          let input_str: String = input.iter().collect();
          let completions = Builtin::TO_STRING
            .into_iter()
            .filter_map(|x| x.strip_prefix(&input_str))
            .collect::<Vec<_>>();
          if completions.len() == 1 {
            let completion = completions.first().unwrap();
            cursor_position += completion.len();
            input.append(&mut completion.chars().collect());

            print!("{completion}");
            std::io::stdout().flush().unwrap();
          }
        }
        CtrlL => {
          input = "clear".chars().collect();
          break;
        }
        CtrlD => {
          println!();
          std::io::stdout().flush().unwrap();
          input = "exit".chars().collect();
          break;
        }
        _ => todo!(),
      }
    }

    let input: String = String::from_iter(input);

    if let Ok(args) = split(&input) {
      let mut args = args.into_iter();
      let Some(command) = args.next() else { continue };
      enum State {
        AppendStdout,
        AppendStderr,
        RedirectStdout,
        RedirectStderr,
        Arg,
      }
      use State::*;
      let mut state = Arg;
      let mut actual_args = vec![];
      let mut stdout: Box<dyn Write> = Box::new(std::io::stdout());
      let mut stderr: Box<dyn Write> = Box::new(std::io::stderr());
      for arg in args {
        state = match state {
          Arg => match arg.as_str() {
            "2>>" => AppendStderr,
            ">>" | "1>>" => AppendStdout,
            ">" | "1>" => RedirectStdout,
            "2>" => RedirectStderr,
            _ => {
              actual_args.push(arg);
              Arg
            }
          },
          AppendStdout => {
            stdout =
              Box::new(std::fs::OpenOptions::new().append(true).create(true).open(arg).unwrap());
            Arg
          }
          AppendStderr => {
            stderr =
              Box::new(std::fs::OpenOptions::new().append(true).create(true).open(arg).unwrap());
            Arg
          }
          RedirectStdout => {
            stdout = Box::new(std::fs::File::create(arg).unwrap());
            Arg
          }
          RedirectStderr => {
            stderr = Box::new(std::fs::File::create(arg).unwrap());
            Arg
          }
        }
      }
      Command::from_split(command, actual_args, &paths).run(
        &paths,
        &mut control_flow,
        stdout,
        stderr,
      );
    } else {
      eprintln!("Syntax error");
      io::stderr().flush().unwrap();
    }
  }
}
