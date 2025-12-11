use std::{
  collections::HashSet,
  fs::{File, OpenOptions},
  io::{self, Read, Write},
  os::{fd::AsRawFd, unix::fs::PermissionsExt},
  path::PathBuf,
  process::{Child, ChildStdout, Stdio},
};

mod split;
use split::*;
mod builtin;
use builtin::*;

fn search(paths: &Vec<PathBuf>, command: &str) -> Option<PathBuf> {
  // PERF: do this using a trie
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

fn executables(paths: &Vec<PathBuf>) -> Vec<String> {
  let mut res = vec![];
  for path in paths {
    if path.is_file() {
      if path.metadata().unwrap().permissions().mode() & 0o111 != 0 {
        res.push(path.file_name().unwrap().to_str().unwrap().to_owned());
      }
    } else if path.is_dir() {
      let entries = std::fs::read_dir(path).unwrap();
      for entry in entries {
        let path = entry.unwrap().path();
        if path.is_file() && path.metadata().unwrap().permissions().mode() & 0o111 != 0 {
          res.push(path.file_name().unwrap().to_str().unwrap().to_owned());
        }
      }
    }
  }

  res
}

#[derive(Debug)]
enum CommandKind {
  Builtin(Builtin),
  Program(PathBuf),
  NotFound(String),
}

impl CommandKind {
  fn parse(command: &str, paths: &Vec<PathBuf>) -> Self {
    let command = command.trim();
    if let Ok(builtin) = command.parse() {
      CommandKind::Builtin(builtin)
    } else if let Some(program) = search(paths, command) {
      CommandKind::Program(program)
    } else {
      CommandKind::NotFound(command.to_owned())
    }
  }
}

pub enum ControlFlow {
  Repl,
  Exit,
}

#[derive(Debug)]
struct Command {
  kind: CommandKind,
  args: Vec<String>,
  stdout: Option<File>,
  stderr: Option<File>,
}

enum CommandOutput {
  Bytes(Vec<u8>),
  Child(Child),
}

impl Command {
  fn from_split(command: String, mut args: Vec<String>, paths: &Vec<PathBuf>) -> Result<Self, ()> {
    let [stdout, stderr] = parse_reditections(&mut args)?;
    Ok(Self { kind: CommandKind::parse(&command, paths), args, stdout, stderr })
  }

  fn run(
    &mut self,
    paths: &Vec<PathBuf>,
    control_flow: &mut ControlFlow,
    stdin: Option<ChildStdout>,
    piped: bool,
  ) -> CommandOutput {
    let Self { kind, args, stdout, stderr } = self;
    match kind {
      CommandKind::Builtin(builtin) => {
        CommandOutput::Bytes(builtin.run(control_flow, stdout, stderr, stdin, paths, args))
      }
      CommandKind::Program(path) => {
        let stdout = match stdout.take() {
          Some(stdout) => Stdio::from(stdout),
          None if piped => Stdio::piped(),
          None => Stdio::inherit(),
        };
        let stderr = match stderr.take() {
          Some(stderr) => Stdio::from(stderr),
          None => Stdio::inherit(),
        };

        let mut cmd = std::process::Command::new(path.file_name().unwrap());
        cmd.args(args);
        cmd.stdout(stdout);
        cmd.stderr(stderr);
        if let Some(stdin) = stdin {
          cmd.stdin(stdin);
        }

        CommandOutput::Child(cmd.spawn().expect("spawn"))
      }
      CommandKind::NotFound(name) => {
        let mut stderr = stderr
          .as_mut()
          .map(|x| Box::new(x) as Box<dyn Write>)
          .unwrap_or(Box::new(std::io::stdout()) as Box<dyn Write>);
        writeln!(stderr, "{name}: command not found").unwrap();
        stderr.flush().unwrap();
        CommandOutput::Bytes(vec![])
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

impl Key {
  fn read_key(mut stdin: &io::Stdin) -> Self {
    let mut byte = [0u8; 1];
    stdin.read_exact(&mut byte).unwrap();
    use Key::*;
    match byte[0] {
      0x08 | 0x7F => Backspace,
      0x0C => CtrlL,
      0x04 => CtrlD,
      0x1B => {
        stdin.read_exact(&mut byte).unwrap();
        if byte[0] == b'[' {
          stdin.read_exact(&mut byte).unwrap();
          match byte[0] {
            b'A' => UpArrow,
            b'B' => DownArrow,
            b'C' => RightArrow,
            b'D' => LeftArrow,
            b'3' => Delete,
            _ => todo!("{}", byte[0]),
          }
        } else {
          todo!();
        }
      }
      b'\t' => Tab,
      b'\n' | b'\r' => Newline,
      ch => Char(ch as char),
    }
  }
}

fn handle_input(stdin: io::Stdin, executables: &[String]) -> String {
  let mut input = Vec::new();
  let mut cursor_position: usize = 0;
  let mut tab_count = 0;

  loop {
    let key = Key::read_key(&stdin);

    use Key::*;
    match key {
      Char(ch) => {
        print!("\x1B[4h{}\x1B[4l", ch);
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
        tab_count = (tab_count + 1) % 2;
        let input_str: String = input.iter().collect();
        let mut completions: HashSet<&str> = HashSet::new();
        completions
          .extend(Builtin::TO_STRING.into_iter().filter_map(|x| x.strip_prefix(&input_str)));
        completions.extend(executables.iter().filter_map(|x| x.strip_prefix(&input_str)));
        let mut completions = Vec::from_iter(completions);
        completions.sort();

        if completions.len() > 1 {
          let first = completions[0];
          let prefix = 'outer: {
            for i in 0..=first.len() {
              if !completions.iter().all(|&s| s.strip_prefix(&first[..i]).is_some()) {
                break 'outer &first[..i - 1];
              }
            }
            first
          };

          if !prefix.is_empty() {
            print!("{prefix}");
            std::io::stdout().flush().unwrap();
            cursor_position += prefix.len();
            input.append(&mut prefix.chars().collect());
          } else if tab_count == 1 {
            print!("\x07");
            std::io::stdout().flush().unwrap();
          } else if tab_count == 0 {
            println!(
              "\n{}",
              completions
                .iter()
                .map(|&x| input.iter().collect::<String>() + x)
                .collect::<Vec<_>>()
                .join("  ")
            );
            print!("$ {}", input.iter().collect::<String>());
            std::io::stdout().flush().unwrap();
          }
        }
        if completions.len() == 1 {
          let completion = completions[0];
          cursor_position += completion.len() + 1;
          input.append(&mut completion.chars().collect());
          input.push(' ');

          print!("{completion} ");
          std::io::stdout().flush().unwrap();
        } else if completions.is_empty() {
          print!("\x07");
          std::io::stdout().flush().unwrap();
        }
      }
      CtrlL => {
        print!("\x1b[1;1H\x1b[0J"); // Clear screen
        print!("$ ");
        print!("{}", String::from_iter(&input));
        std::io::stdout().flush().unwrap();
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

  String::from_iter(input)
}

fn parse_reditections(args_vec: &mut Vec<String>) -> Result<[Option<File>; 2], ()> {
  let mut args = args_vec.iter();
  let mut stdout = None;
  let mut stderr = None;
  let mut append = OpenOptions::new();
  let mut actual_args = vec![];
  append.append(true).create(true);

  while let Some(arg) = args.next() {
    match arg.as_str() {
      ">" | "1>" => stdout = Some(File::create(args.next().ok_or(())?).map_err(|_| ())?),
      "2>" => stderr = Some(File::create(args.next().ok_or(())?).map_err(|_| ())?),
      ">>" | "1>>" => stdout = Some(append.open(args.next().ok_or(())?).map_err(|_| ())?),
      "2>>" => stderr = Some(append.open(args.next().ok_or(())?).map_err(|_| ())?),
      _ => {
        actual_args.push(arg.clone()); // PERF: this is a bit wastefull
        continue;
      }
    }
  }

  *args_vec = actual_args;
  Ok([stdout, stderr])
}

fn main() {
  let path = std::env::var("PATH").unwrap();
  let paths: Vec<_> = std::env::split_paths(&path).collect();
  let executables = executables(&paths);
  let mut control_flow = ControlFlow::Repl;

  let fd = io::stdin().as_raw_fd();
  let mut termios = unsafe {
    let mut t = std::mem::zeroed();
    libc::tcgetattr(fd, &mut t);
    t
  };

  let original_termios = termios;
  termios.c_lflag &= !(libc::ECHO | libc::ICANON);

  unsafe {
    libc::tcsetattr(fd, libc::TCSANOW, &termios);
  }

  while let ControlFlow::Repl = &control_flow {
    print!("$ ");
    io::stdout().flush().unwrap();

    let input: String = handle_input(io::stdin(), &executables);
    let mut command_strings = input.split("|").peekable();

    let mut stdin = None;
    let mut child_handles = vec![];
    while let Some(command_string) = command_strings.next() {
      if let Ok(mut args) = split(command_string) {
        let command = if !args.is_empty() { args.remove(0) } else { continue };
        let Ok(mut cmd) = Command::from_split(command, args, &paths) else { continue };
        let piped = command_strings.peek().is_some();

        stdin = match cmd.run(&paths, &mut control_flow, stdin, piped) {
          CommandOutput::Bytes(_) => None,
          CommandOutput::Child(child) => {
            child_handles.push(child);
            let len = child_handles.len();
            if piped { child_handles[len - 1].stdout.take() } else { break }
          }
        }
      } else {
        eprintln!("Syntax error");
        io::stderr().flush().unwrap();
        break;
      }
    }

    while let Some(child) = child_handles.pop() {
      child.wait_with_output().expect("complete");
    }
  }

  unsafe {
    libc::tcsetattr(fd, libc::TCSANOW, &original_termios);
  }
}
