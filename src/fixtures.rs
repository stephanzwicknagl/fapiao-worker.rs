use jiff::{ToSpan, civil::Date};
use pdf_oxide::document::PdfDocument;
use pdf_oxide::writer::DocumentBuilder;

use crate::Result;
use crate::model::{Fapiao, LocatedFapiao};
fn base() -> Fapiao {
  Fapiao {
    fapiao_number: Some("12345678".into()),
    date: None,
    amount: None,
    vat_amount: None,
    seller: None,
    products: None,
    skip: false,
    skip_reason: None,
  }
}
pub fn with_number(n: &str) -> Fapiao {
  Fapiao {
    fapiao_number: Some(n.into()),
    ..base()
  }
}
pub fn without_number() -> Fapiao {
  Fapiao {
    fapiao_number: None,
    ..base()
  }
}

pub fn skipped(reason: &str) -> Fapiao {
  Fapiao {
    skip: true,
    skip_reason: Some(reason.into()),
    ..base()
  }
}

pub fn full_fapiao(i: usize) -> Fapiao {
  let n = 10e13 as usize + i;
  Fapiao {
    fapiao_number: Some(format!("{n}").to_string()),
    date: Some(Date::new(2024, 3, 20).unwrap() + (i as i32).days()),
    amount: Some(123 as f32),
    vat_amount: Some(45 as f32),
    seller: Some("asdf".to_string()),
    products: None,
    skip: false,
    skip_reason: None,
  }
}

pub fn many(n: usize) -> Vec<LocatedFapiao> {
  (0..n)
    .rev()
    .enumerate()
    .map(|(j, i)| LocatedFapiao {
      fapiao: full_fapiao(i),
      page_number: 1,
      doc_number: j,
      page_amount: 1,
    })
    .collect()
}

pub fn mixed() -> Vec<Fapiao> {
  vec![skipped("duplicate"), without_number(), with_number("AAA")]
}

pub fn create_pdf(fapiao: LocatedFapiao) -> PdfDocument {
  let mut b = DocumentBuilder::new();
  let page = b.a4_page();
  page
    .font("Helvetica", 12.0)
    .text(
      format!(
        "发票号码：{}",
        fapiao.fapiao.fapiao_number.unwrap_or_default()
      )
      .as_str(),
    )
    .text(
      format!(
        "开票日期：{}年{}月{}日",
        fapiao.fapiao.date.unwrap_or_default().year(),
        fapiao.fapiao.date.unwrap_or_default().month(),
        fapiao.fapiao.date.unwrap_or_default().day()
      )
      .as_str(),
    )
    .text(format!("PLACEMENT-MARKER-----PLACED-{}th", fapiao.doc_number).as_str())
    .text(format!("（小写）¥{}", fapiao.fapiao.amount.unwrap_or_default()).as_str())
    .done();
  let bytes = b.build().unwrap();
  PdfDocument::from_bytes(bytes).unwrap()
}

pub fn create_input_files(n: usize) -> Vec<PdfDocument> {
  many(n).into_iter().map(|f| create_pdf(f)).collect()
}
