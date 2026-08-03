use simple_datetime_rs::Date;
#[derive(Debug)]
#[allow(dead_code)]
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
