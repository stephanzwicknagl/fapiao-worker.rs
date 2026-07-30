use lopdf::Document;
use regex::Regex;
use simple_datetime_rs::DateTime;

use crate::Error;
use crate::Result;

pub fn extract(bytes: &[u8]) -> Result<String> {
  let doc = Document::load_from(bytes)?;
  let pages = doc.get_pages();
  let mut fapiaos: Vec<Fapiao> = vec![];
  for (i, _) in pages {
    let text = doc.extract_text(&[i])?;
    fapiaos.push(make_fapiao(text)?);
  }
  Ok(format!("Received Fapiaos: {:?}", fapiaos))
}

fn make_fapiao(text: String) -> Result<Fapiao> {
  // GARBLED DETECTION
  // All valid fapiaos have a date with 年
  let re = Regex::new("年")?;
  if !re.is_match(&text) {
    return Ok(Fapiao {
      fapiao_number: None,
      date: None,
      amount: None,
      vat_amount: None,
      products: None,
      skip: true,
      skip_reason: Some("garbled text".to_string()),
    });
  }

  // ── SKIP CONTINUATION PAGES of multi-page fapiaos ────────────────────────
  let re = Regex::new(r"共\s*(\d+)\s*页\s*第\s*(\d+)\s*页")?;
  if let Some(caps) = re.captures(&text)
    && let Some(total) = caps.get(0)
    && let Some(current) = caps.get(1)
  {
    println!("parsed {}", total.as_str());
    if total.as_str().parse::<i32>()? > current.as_str().parse::<i32>()? {
      return Ok(Fapiao {
        fapiao_number: None,
        date: None,
        amount: None,
        vat_amount: None,
        products: None,
        skip: true,
        skip_reason: Some(format!("page {} of {}", current.as_str(), total.as_str())),
      });
    }
  }
  // Err(Error::FapiaoParseError(0))

  Ok(Fapiao {
    fapiao_number: None,
    date: None,
    amount: None,
    vat_amount: None,
    products: None,
    skip: false,
    skip_reason: None,
  })
}

#[derive(Debug)]
struct Fapiao {
  fapiao_number: Option<String>,
  date: Option<DateTime>,
  amount: Option<f32>,
  vat_amount: Option<f32>,
  products: Option<Vec<String>>,
  skip: bool,
  skip_reason: Option<String>,
}
