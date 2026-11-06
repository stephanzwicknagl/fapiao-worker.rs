use pdf_oxide::PdfDocument;
use pdf_oxide::editor::DocumentEditor;

use crate::Result;
use crate::model::LocatedFapiao;

pub fn merge_sorted_pdfs(
  pdf_docs: Vec<PdfDocument>,
  fapiaos: &Vec<LocatedFapiao>,
) -> Result<Vec<u8>> {
  let mut acc: Option<DocumentEditor> = None;
  let mut pdf_editors: Vec<DocumentEditor> = pdf_docs
    .into_iter()
    .map(|doc| DocumentEditor::from_document(doc))
    .collect::<Result<_, _>>()?;

  let fapiaos_for_filling: Vec<&LocatedFapiao> =
    fapiaos.iter().filter(|f| !f.fapiao.skip).collect();
  for f in fapiaos_for_filling {
    let pages: Vec<usize> = ((f.page_number - f.page_amount + 1)..f.page_number).collect();
    let pdf_pages = pdf_editors[f.doc_number].extract_pages_to_bytes(&pages)?;
    if let Some(acc) = &mut acc {
      acc.merge_from_bytes(&pdf_pages.into_boxed_slice())?;
    } else {
      // Initialize
      acc = Some(DocumentEditor::from_bytes(pdf_pages)?);
    }
  }

  let r = acc.unwrap().save_to_bytes()?;
  Ok(r)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::extract::extract;
  use crate::fixtures::{create_input_files, full_fapiao};

  #[test]
  fn test_merged_fapiaos() -> Result<()> {
    let docs = create_input_files(3);
    let fapiaos = extract(&docs)?;
    let merged = merge_sorted_pdfs(docs, &fapiaos)?;

    let read_merged = PdfDocument::from_bytes(merged)?;
    assert_eq!(read_merged.page_count()?, 3);

    // how to assert right order, if I don't know right order from the create_input_files?
    Ok(())
  }

  #[test]
  fn test_multi_page_fapiaos_all_pages_are_preserved() -> Result<()> {
    todo!()
  }
}
