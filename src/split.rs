use std::mem;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError;

enum State {
  Delimiter,
  SingleQuoted,
  Unquoted,
}

pub fn split(s: &str) -> Result<Vec<String>, ParseError> {
  use State::*;
  let mut state = Delimiter;
  let mut words = vec![];
  let mut word = String::new();
  let mut chars = s.chars();

  loop {
    let c = chars.next();
    state = match state {
      Delimiter => match c {
        None => break,
        Some('\'') => SingleQuoted,
        Some(w) if w.is_whitespace() => Delimiter,
        Some(c) => {
          word.push(c);
          Unquoted
        }
      },
      Unquoted => match c {
        None => {
          words.push(mem::take(&mut word));
          break;
        }
        Some('\'') => SingleQuoted,
        Some(w) if w.is_whitespace() => {
          words.push(mem::take(&mut word));
          Delimiter
        }
        Some(c) => {
          word.push(c);
          Unquoted
        }
      },
      SingleQuoted => match c {
        None => return Err(ParseError),
        Some('\'') => Unquoted,
        Some(c) => {
          word.push(c);
          SingleQuoted
        }
      },
    }
  }

  Ok(words)
}
