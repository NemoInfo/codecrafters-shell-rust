use std::io::{self, Write};

fn main() {
  loop {
    print!("$ ");
    io::stdout().flush().unwrap();
    let mut command = String::new();
    io::stdin().read_line(&mut command).expect("Expected command");
    println!("{}: command not found", command.trim());
    io::stdout().flush().unwrap();
  }
}
