use crate::Result;
use crate::model::Fapiao;

pub fn place_fapiaos_in_xlsx(fapiaos: Vec<Fapiao>, excel_bytes: Vec<u8>) -> Result<Vec<u8>> {
  todo!()
}

#[cfg(test)]
mod tests {
  use simple_datetime_rs::Format;

  use super::*;
  use crate::extract::extract;

  #[test]
  fn writes_to_sample_xlsx() -> Result<()> {
    let bytes_xlsx = include_bytes!("../fixtures/sample.xlsx");
    let bytes_pdf = include_bytes!("../fixtures/sample.pdf");
    let fapiaos = extract(vec![bytes_pdf.to_vec()])?;
    let out = place_fapiaos_in_xlsx(fapiaos, bytes_xlsx.to_vec())?;
    println!("{:#?}", out);
    Ok(())
  }
}
