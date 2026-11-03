use jiff::civil::Date;
#[derive(Debug, Clone)]
pub struct Fapiao {
  pub fapiao_number: Option<String>,
  pub date: Option<Date>,
  pub amount: Option<f32>,
  pub vat_amount: Option<f32>,
  pub seller: Option<String>,
  pub products: Option<Vec<(String, String)>>,
  pub skip: bool,
  pub skip_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct LocatedFapiao {
  pub fapiao: Fapiao,
  pub doc_number: usize,
  pub page_number: usize,
  pub page_amount: usize,
}

impl PartialOrd for Fapiao {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Fapiao {
  fn cmp(&self, other: &Self) -> core::cmp::Ordering {
    self.date.cmp(&other.date).then_with(|| {
      other
        .amount
        .unwrap_or(0.0 as f32)
        .total_cmp(&self.amount.unwrap_or(0.0 as f32))
    })
  }
}

impl PartialEq for Fapiao {
  fn eq(&self, other: &Self) -> bool {
    self.fapiao_number.eq(&other.fapiao_number) && self.date.eq(&other.date)
  }
}

impl Eq for Fapiao {}
