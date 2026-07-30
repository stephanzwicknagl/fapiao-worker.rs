mod extract;

use std::io;
use thiserror::Error;
use wasm_bindgen::prelude::*;

type Result<T, E = Error> = std::result::Result<T, E>;

#[wasm_bindgen]
pub fn greet(name: &str) -> String {
  format!("Hello from Rust, {name}!")
}

#[wasm_bindgen]
pub fn parse_pdf(bytes: &[u8]) -> Result<String, JsError> {
  let text = extract::extract(bytes)?;
  Ok(text)
}

#[derive(Debug, Error)]
enum Error {
  #[error(transparent)]
  IoError(#[from] io::Error),

  #[error(transparent)]
  GitError(#[from] lopdf::Error),

  #[error(transparent)]
  RegexError(#[from] regex::Error),

  #[error("can't parse fapiao from page {0}'")]
  FapiaoParseError(u32),

  #[error(transparent)]
  ParseIntError(#[from] std::num::ParseIntError),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn it_works() {
    let result = greet("lorem");
    assert_eq!(result, "Hello from Rust, lorem!");
  }
}
