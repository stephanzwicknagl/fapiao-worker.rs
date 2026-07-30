use lopdf::Document;
use simple_datetime_rs::DateTime;

use crate::Result;

pub fn extract(bytes: &[u8]) -> Result<String> {
  let doc = Document::load_from(bytes)?;
  let pages = doc.get_pages();
  let mut fapiaos: Vec<Fapiao> = vec![];
  for (i, _) in pages {
    let text = doc.extract_text(&[i])?;
    fapiaos.push(make_fapiao(text)?);
  }
  Ok(format!("Received Fapiaos: {}", fapiaos.len()))
}

fn make_fapiao(text: String) -> Result<Fapiao> {
  todo!()
}

struct Fapiao {
  fapiao_number: Option<String>,
  date: Option<DateTime>,
  amount: Option<f32>,
  vat_amount: Option<f32>,
  products: Option<Vec<String>>,
  skip: bool,
  skip_reason: Option<String>,
}
