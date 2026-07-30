use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn greet(name: &str) -> String {
  format!("Hello from Rust, {name}!")
}

#[wasm_bindgen]
pub fn parse_pdf(bytes: &[u8]) -> String {
  format!("Got {} bytes", bytes.len())
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
