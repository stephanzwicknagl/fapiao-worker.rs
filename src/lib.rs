mod extract;
mod fill;
mod merge;
mod model;

#[cfg(test)]
mod fixtures;

use pdf_oxide::PdfDocument;
use std::io;
use thiserror::Error;
use wasm_bindgen::prelude::*;

type Result<T, E = Error> = std::result::Result<T, E>;

#[wasm_bindgen(start)]
pub fn init() {
  console_log::init_with_level(log::Level::Debug).unwrap();
}

#[wasm_bindgen]
pub struct FillResult {
  bytes_filled: js_sys::Uint8Array,
  bytes_sorted: js_sys::Uint8Array,
  warnings: Vec<String>,
}

#[wasm_bindgen]
impl FillResult {
  #[wasm_bindgen(getter)]
  pub fn bytes_filled(&self) -> js_sys::Uint8Array {
    self.bytes_filled.clone()
  }

  #[wasm_bindgen(getter)]
  pub fn bytes_sorted(&self) -> js_sys::Uint8Array {
    self.bytes_sorted.clone()
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
  let doc = bytes_vec
    .iter()
    .map(|a| PdfDocument::from_bytes(a.to_vec()))
    .collect::<Result<_, _>>()?;
  let fapiaos = extract::extract(&doc)?;
  Ok(format!("EXTRACTED FAPIOS: {:#?}", fapiaos))
}

#[wasm_bindgen]
pub fn merge_sorted_pdfs(
  pdf_files_bytes: Vec<js_sys::Uint8Array>,
) -> Result<js_sys::Uint8Array, JsError> {
  let pdf_docs = pdf_files_bytes
    .iter()
    .map(|a| PdfDocument::from_bytes(a.to_vec()))
    .collect::<Result<_, _>>()?;
  let fapiaos = extract::extract(&pdf_docs)?;
  let merged_pdf = merge::merge_sorted_pdfs(pdf_docs, &fapiaos)?;
  Ok(js_sys::Uint8Array::from(merged_pdf.as_slice()))
}

#[wasm_bindgen]
pub fn fill_claim_form(
  pdf_files_bytes: Vec<js_sys::Uint8Array>,
  xlsx_bytes: js_sys::Uint8Array,
) -> Result<FillResult, JsError> {
  let pdf_docs: Vec<PdfDocument> = pdf_files_bytes
    .iter()
    .map(|a| PdfDocument::from_bytes(a.to_vec()))
    .collect::<Result<_, _>>()?;
  let fapiaos = extract::extract(&pdf_docs)?;

  let warnings: Vec<String> = fapiaos
    .iter()
    .filter(|f| f.fapiao.skip)
    .filter_map(|f| f.fapiao.skip_reason.clone())
    .collect();

  let filled_spreadsheet = fill::insert_fapiao_info_in_xlsx(xlsx_bytes.to_vec(), &fapiaos)?;
  let merged_pdf = merge::merge_sorted_pdfs(pdf_docs, &fapiaos)?;
  Ok(FillResult {
    bytes_filled: js_sys::Uint8Array::from(filled_spreadsheet.as_slice()),
    bytes_sorted: js_sys::Uint8Array::from(merged_pdf.as_slice()),
    warnings,
  })
}

#[derive(Debug, Error)]
enum Error {
  #[error(transparent)]
  IoError(#[from] io::Error),

  #[error(transparent)]
  RegexError(#[from] regex::Error),

  // #[error("can't parse fapiao from page {0}'")]
  // FapiaoParseError(u32),
  #[error(transparent)]
  ParseIntError(#[from] std::num::ParseIntError),

  #[error(transparent)]
  DateError(#[from] jiff::Error),

  #[error(transparent)]
  ParseFloatError(#[from] std::num::ParseFloatError),

  #[error(transparent)]
  XlsxError(#[from] umya_spreadsheet::XlsxError),

  #[error(transparent)]
  ZipError(#[from] zip::result::ZipError),

  #[error(transparent)]
  PdfError(#[from] pdf_oxide::Error),

  #[error("no readable fapiao pages found")]
  NoFapiaoPages,
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
