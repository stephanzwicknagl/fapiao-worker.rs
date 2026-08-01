mod extract;
mod fill;
mod model;

use std::io;
use thiserror::Error;
use wasm_bindgen::prelude::*;

type Result<T, E = Error> = std::result::Result<T, E>;

#[wasm_bindgen]
pub struct FillResult {
  bytes: js_sys::Uint8Array,
  warnings: Vec<String>,
}

#[wasm_bindgen]
impl FillResult {
  #[wasm_bindgen(getter)]
  pub fn bytes(&self) -> js_sys::Uint8Array {
    self.bytes.clone()
  }

  #[wasm_bindgen(getter)]
  pub fn warnings(&self) -> Vec<String> {
    self.warnings.clone()
  }
}
#[wasm_bindgen]
pub fn greet(name: &str) -> String {
  format!("Hello from Rust, {name}!")
}

#[wasm_bindgen]
pub fn parse_pdf(bytes_vec: Vec<js_sys::Uint8Array>) -> Result<String, JsError> {
  let bytes_vec = bytes_vec.iter().map(|a| a.to_vec()).collect();
  let fapiaos = extract::extract(bytes_vec)?;
  Ok(format!("EXTRACTED FAPIOS: {:#?}", fapiaos))
}

#[wasm_bindgen]
pub fn fill_claim_form(
  pdf_bytes_vec: Vec<js_sys::Uint8Array>,
  xlsx_bytes: js_sys::Uint8Array,
) -> Result<FillResult, JsError> {
  let pdf_bytes_vec = pdf_bytes_vec.iter().map(|a| a.to_vec()).collect();
  let fapiaos = extract::extract(pdf_bytes_vec)?;

  let warnings: Vec<String> = fapiaos
    .iter()
    .filter(|f| f.skip)
    .filter_map(|f| f.skip_reason.clone())
    .collect();

  let out = fill::place_fapiaos_in_xlsx(fapiaos, xlsx_bytes.to_vec())?;
  Ok(FillResult {
    bytes: js_sys::Uint8Array::from(out.as_slice()),
    warnings,
  })
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

  #[error(transparent)]
  DateError(#[from] simple_datetime_rs::DateError),

  #[error(transparent)]
  ParseFloatError(#[from] std::num::ParseFloatError),
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
