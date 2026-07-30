use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn greet(name: &str) -> String {
  format!("Hello from Rust, {name}!")
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
