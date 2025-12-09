use std::io::{self, Write};

fn main() {
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
          "echo" | "exit" => {
            println!("{command} is a shell builtin");
            io::stdout().flush().unwrap();
          }
          _ => {
            println!("{command}: command not found");
            io::stdout().flush().unwrap();
          }
        }
      }
      invalid => {
        println!("{}: command not found", invalid);
        io::stdout().flush().unwrap();
      }
    }
  }
}
