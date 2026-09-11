use std::io::{Cursor, Read, Write};

use crate::Result;
use crate::model::Fapiao;

use jiff::civil::time;
use umya_spreadsheet::Workbook;
use umya_spreadsheet::helper::date::jiff_date_time_to_excel;
use umya_spreadsheet::reader::xlsx::read_reader;
use umya_spreadsheet::writer::xlsx::write_writer;

pub fn insert_fapiao_info_in_xlsx(excel_bytes: Vec<u8>, fapiaos: Vec<Fapiao>) -> Result<Vec<u8>> {
  let book = read_reader(Cursor::new(excel_bytes), true)?;
  let mut out: Vec<u8> = Vec::new();
  write_writer(&edit_xlsx(book, fapiaos)?, &mut out)?;
  force_full_formula_recalc_on_next_load(out)
}

fn force_full_formula_recalc_on_next_load(xlsx_bytes: Vec<u8>) -> Result<Vec<u8>> {
  let mut archive = zip::ZipArchive::new(Cursor::new(xlsx_bytes))?;
  let mut out = Cursor::new(Vec::new());
  {
    let mut writer = zip::ZipWriter::new(&mut out);
    for i in 0..archive.len() {
      let mut file = archive.by_index(i)?;
      let options = zip::write::SimpleFileOptions::default().compression_method(file.compression());
      if file.name() == "xl/workbook.xml" {
        let mut xml = String::new();
        file.read_to_string(&mut xml)?;
        writer.start_file(file.name(), options)?;
        writer.write_all(enable_full_calc_on_load(&xml).as_bytes())?;
      } else {
        writer.raw_copy_file(file)?;
      }
    }
    writer.finish()?;
  }
  Ok(out.into_inner())
}

fn enable_full_calc_on_load(workbook_xml: &str) -> String {
  if workbook_xml.contains("fullCalcOnLoad=\"1\"") {
    return workbook_xml.to_string();
  }
  let mut xml = workbook_xml.to_string();
  if let Some(pos) = xml.find("<calcPr") {
    xml.insert_str(pos + "<calcPr".len(), " fullCalcOnLoad=\"1\"");
  } else if let Some(pos) = xml.find("</workbook>") {
    xml.insert_str(pos, "<calcPr fullCalcOnLoad=\"1\"/>");
  }
  xml
}

fn edit_xlsx(mut book: Workbook, fapiaos: Vec<Fapiao>) -> Result<Workbook> {
  let sheet = book.active_sheet_mut();
  let mut row = 12;
  let fapiaos_for_filling: Vec<&Fapiao> = fapiaos.iter().filter(|f| !f.skip).collect();
  sheet
    .cell_mut((1, 10))
    .set_formula_result_number(fapiaos_for_filling.len() as f64);
  for fapiao in &fapiaos_for_filling {
    if let Some(d) = &fapiao.date
      && let Some(n) = &fapiao.fapiao_number
      && let Some(a) = &fapiao.amount
      && let Some(vat_a) = &fapiao.vat_amount
    {
      let date = jiff_date_time_to_excel(d.to_datetime(time(0, 0, 0, 0)));

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
  let total_sum = fapiaos_for_filling
    .iter()
    .rfold(0.0, |s, f| s + f.amount.unwrap_or(0.0));
  sheet
    .cell_mut((9, 52))
    .set_formula_result_number(total_sum as f64);
  let vat_sum = fapiaos_for_filling
    .iter()
    .rfold(0.0, |s, f| s + f.vat_amount.unwrap_or(0.0));
  sheet
    .cell_mut((10, 52))
    .set_formula_result_number(vat_sum as f64);

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
  fn forces_full_recalculation_on_load() -> Result<()> {
    let bytes_xlsx = include_bytes!("../fixtures/1.xlsx");
    let fapiaos = fixtures::many(3);
    let out = insert_fapiao_info_in_xlsx(bytes_xlsx.to_vec(), fapiaos)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(out))?;
    let mut xml = String::new();
    archive
      .by_name("xl/workbook.xml")?
      .read_to_string(&mut xml)?;
    assert!(xml.contains("fullCalcOnLoad=\"1\""));
    Ok(())
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
      let found_date: jiff::civil::Date =
        excel_to_date_time_jiff(sheet.cell_value((2, i as u32 + 12)).value_number().unwrap())
          .into();
      assert_eq!(found_date, f.date.unwrap());
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
