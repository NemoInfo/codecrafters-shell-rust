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
      invalid => {
        io::stdout().flush().unwrap();
        println!("{}: command not found", invalid);
      }
    }
  }
}
