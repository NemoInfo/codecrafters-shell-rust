use std::io::{self, Write};

fn main() {
  loop {
    print!("$ ");
    io::stdout().flush().unwrap();
    let mut command = String::new();
    io::stdin().read_line(&mut command).expect("Expected command");
    match command.trim() {
      "exit" => {break}
      invalid => {
        io::stdout().flush().unwrap();
        println!("{}: command not found", invalid);
      }
    }
  }
}
