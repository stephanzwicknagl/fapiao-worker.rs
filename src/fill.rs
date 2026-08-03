use std::io::Cursor;

use crate::Result;
use crate::model::Fapiao;

use umya_spreadsheet::Workbook;
use umya_spreadsheet::reader::xlsx::read_reader;
use umya_spreadsheet::writer::xlsx::write_writer;

pub fn insert_fapiao_info_in_xlsx(excel_bytes: Vec<u8>, fapiaos: Vec<Fapiao>) -> Result<Vec<u8>> {
  let book = read_reader(Cursor::new(excel_bytes), true)?;
  let mut out: Vec<u8> = Vec::new();
  write_writer(&edit_xlsx(book, fapiaos)?, &mut out)?;
  Ok(out)
}

fn edit_xlsx(mut book: Workbook, fapiaos: Vec<Fapiao>) -> Result<Workbook> {
  let sheet = book.active_sheet_mut();
  for (i, fapiao) in fapiaos.iter().filter(|f| !f.skip).enumerate() {
    if let Some(number) = &fapiao.fapiao_number {
      sheet
        .cell_mut(format!("C{}", i + 12))
        .set_value_string(number);
    }
  }
  Ok(book)
}

#[cfg(test)]
mod tests {
  use simple_datetime_rs::Format;

  use super::*;
  use crate::extract::extract;

  #[test]
  fn writes_to_sample_xlsx() -> Result<()> {
    let bytes_xlsx = include_bytes!("../fixtures/1.xlsx");
    let bytes_pdf = include_bytes!("../fixtures/sample.pdf");
    let fapiaos = extract(vec![bytes_pdf.to_vec()])?;
    let out = insert_fapiao_info_in_xlsx(bytes_xlsx.to_vec(), fapiaos)?;
    println!("{:#?}", out);
    Ok(())
  }
}
