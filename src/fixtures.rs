use jiff::{ToSpan, civil::Date};

use crate::model::Fapiao;
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

pub fn many(n: usize) -> Vec<Fapiao> {
  (0..n).rev().map(|i| full_fapiao(i)).collect()
}

pub fn mixed() -> Vec<Fapiao> {
  vec![skipped("duplicate"), without_number(), with_number("AAA")]
}
