use std::io::Cursor;

use crate::Result;
use crate::model::Fapiao;

use umya_spreadsheet::Workbook;
use umya_spreadsheet::helper::date::convert_date;
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
  let mut row = 12;
  for fapiao in fapiaos.iter().filter(|f| !f.skip) {
    if let Some(d) = &fapiao.date
      && let Some(n) = &fapiao.fapiao_number
      && let Some(a) = &fapiao.amount
      && let Some(vat_a) = &fapiao.vat_amount
    {
      let date = convert_date(d.year as i32, d.month as i32, d.day as i32, 0, 0, 0);

      sheet.cell_mut((2, row)).set_value_number(date);
      sheet.cell_mut((3, row)).set_value_string(n);
      sheet.cell_mut((7, row)).set_value("1");
      sheet.cell_mut((9, row)).set_value_number(*a);
      sheet.cell_mut((10, row)).set_value_number(*vat_a);
      row = row + 1;
    } else {
      sheet.cell_mut((11, row)).set_value_string("Skipped");
      row = row + 1;
    }
  }
  Ok(book)
}

#[cfg(test)]
mod tests {
  use umya_spreadsheet::CellRawValue;
  use umya_spreadsheet::helper::date::excel_to_date_time_jiff;

  use super::*;
  use crate::fixtures;

  #[test]
  fn errors_on_invalid_xlsx_bytes() {
    let fake_bytes = b"not an xlsx".to_vec();
    let fapiaos = fixtures::many(1);
    let result = insert_fapiao_info_in_xlsx(fake_bytes, fapiaos);
    assert!(result.is_err());
  }

  #[test]
  fn writes_fapiao_number() -> Result<()> {
    let bytes_xlsx = include_bytes!("../fixtures/1.xlsx");
    let fapiaos = fixtures::many(3);
    let out = insert_fapiao_info_in_xlsx(bytes_xlsx.to_vec(), fapiaos.clone())?;
    let book = read_reader(Cursor::new(out), true)?;
    let sheet = book.active_sheet();
    for (i, f) in fapiaos.iter().enumerate() {
      assert_eq!(sheet.cell_value((3, i as u32 + 12)).data_type(), "s");
      assert_eq!(
        sheet.cell_value((3, i as u32 + 12)).raw_value(),
        &CellRawValue::String(f.fapiao_number.as_ref().unwrap().clone().into())
      );
    }
    Ok(())
  }

  #[test]
  fn writes_fapiao_date() -> Result<()> {
    let bytes_xlsx = include_bytes!("../fixtures/1.xlsx");
    let fapiaos = fixtures::many(3);
    let out = insert_fapiao_info_in_xlsx(bytes_xlsx.to_vec(), fapiaos.clone())?;
    let book = read_reader(Cursor::new(out), true)?;
    let sheet = book.active_sheet();
    for (i, f) in fapiaos.iter().enumerate() {
      assert_eq!(sheet.cell_value((2, i as u32 + 12)).data_type(), "n");
      let found_date =
        excel_to_date_time_jiff(sheet.cell_value((2, i as u32 + 12)).value_number().unwrap());
      // assert_eq!(found_date.year(), f.date.unwrap().year.try_into().unwrap());
      // assert_eq!(
      //   found_date.month(),
      //   f.date.unwrap().month.try_into().unwrap()
      // );
      // assert_eq!(found_date.day(), f.date.unwrap().day.try_into().unwrap());
    }
    Ok(())
  }

  #[test]
  fn writes_quantity() -> Result<()> {
    let bytes_xlsx = include_bytes!("../fixtures/1.xlsx");
    let fapiaos = fixtures::many(3);
    let out = insert_fapiao_info_in_xlsx(bytes_xlsx.to_vec(), fapiaos.clone())?;
    let book = read_reader(Cursor::new(out), true)?;
    let sheet = book.active_sheet();
    for (i, _) in fapiaos.iter().enumerate() {
      assert_eq!(sheet.cell_value((7, i as u32 + 12)).data_type(), "n");
      assert_eq!(
        sheet.cell_value((7, i as u32 + 12)).raw_value(),
        &CellRawValue::Numeric(1 as f64)
      );
    }
    Ok(())
  }

  #[test]
  fn writes_amount() -> Result<()> {
    let bytes_xlsx = include_bytes!("../fixtures/1.xlsx");
    let fapiaos = fixtures::many(3);
    let out = insert_fapiao_info_in_xlsx(bytes_xlsx.to_vec(), fapiaos.clone())?;
    let book = read_reader(Cursor::new(out), true)?;
    let sheet = book.active_sheet();
    for (i, f) in fapiaos.iter().enumerate() {
      assert_eq!(sheet.cell_value((9, i as u32 + 12)).data_type(), "n");
      assert_eq!(
        sheet.cell_value((9, i as u32 + 12)).raw_value(),
        &CellRawValue::Numeric(f.amount.unwrap().into())
      );
    }
    Ok(())
  }

  #[test]
  fn writes_vat_amount() -> Result<()> {
    let bytes_xlsx = include_bytes!("../fixtures/1.xlsx");
    let fapiaos = fixtures::many(3);
    let out = insert_fapiao_info_in_xlsx(bytes_xlsx.to_vec(), fapiaos.clone())?;
    let book = read_reader(Cursor::new(out), true)?;
    let sheet = book.active_sheet();
    for (i, f) in fapiaos.iter().enumerate() {
      assert_eq!(sheet.cell_value((10, i as u32 + 12)).data_type(), "n");
      assert_eq!(
        sheet.cell_value((10, i as u32 + 12)).raw_value(),
        &CellRawValue::Numeric(f.vat_amount.unwrap().into())
      );
    }
    Ok(())
  }

  #[test]
  fn leaves_remarks_empty() -> Result<()> {
    let bytes_xlsx = include_bytes!("../fixtures/1.xlsx");
    let fapiaos = fixtures::many(3);
    let out = insert_fapiao_info_in_xlsx(bytes_xlsx.to_vec(), fapiaos.clone())?;
    let book = read_reader(Cursor::new(out), true)?;
    let sheet = book.active_sheet();
    for (i, _) in fapiaos.iter().enumerate() {
      assert_eq!(sheet.cell_value((11, i as u32 + 12)).data_type(), "");
      assert_eq!(
        sheet.cell_value((11, i as u32 + 12)).raw_value(),
        &CellRawValue::Empty
      );
    }
    Ok(())
  }
}
