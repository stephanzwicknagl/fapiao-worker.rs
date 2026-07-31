use lopdf::Document;
use regex::Regex;
use simple_datetime_rs::Date;

use crate::Result;

const NUM: &'static str = r"[\d,]+\.?\d*";
const DAXIE: &'static str =
  r"[壹贰叁肆伍陆柒捌玖拾零百千万亿佰仟][壹贰叁肆伍陆柒捌玖拾零百千万亿佰仟圆元角分整]{2,}[整]?";

pub fn extract(bytes: &[u8]) -> Result<String> {
  let doc = Document::load_from(bytes)?;
  let pages = doc.get_pages();
  let mut fapiaos: Vec<Fapiao> = vec![];
  for (i, _) in pages {
    let text = doc.extract_text(&[i])?;
    fapiaos.push(parse_fapiao(text)?);
  }
  Ok(format!("Received Fapiaos: {:?}", fapiaos))
}

fn parse_fapiao(text: String) -> Result<Fapiao> {
  // Normalize full-width characters to ASCII
  let text = normalize_fullwidth(&text);

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

  // Detect railway e-tickets for special VAT handling
  let is_railway_ticket =
    text.contains("铁路电子客票") || (text.contains("中国铁路") && text.contains("买票请到"));

  // ── SKIP CONTINUATION PAGES of multi-page fapiaos ────────────────────────
  let re = Regex::new(r"共\s*(\d+)\s*页\s*第\s*(\d+)\s*页")?;
  if let Some(caps) = re.captures(&text)
    && let Some(total) = caps.get(1)
    && let Some(current) = caps.get(2)
  {
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
  let mut fapiao = Fapiao {
    fapiao_number: None,
    date: None,
    amount: None,
    vat_amount: None,
    products: None,
    skip: false,
    skip_reason: None,
  };
  //── FAPIAO NUMBER ──────────────────────────────────────────────────────────
  let re = Regex::new(r"发票号码[：:]\s*(\d{15,})")?;
  if let Some(caps) = re.captures(&text)
    && let Some(f_number) = caps.get(1)
  {
    fapiao.fapiao_number = Some(f_number.as_str().to_string());
  } else {
    let re = Regex::new(r"\b(\d{20})\b")?;
    if let Some(caps) = re.captures(&text)
      && let Some(f_number) = caps.get(1)
    {
      fapiao.fapiao_number = Some(f_number.as_str().to_string());
    }
  }

  // ── DATE ───────────────────────────────────────────────────────────────────
  // Prefer 开票日期 (invoice issue date) if available - this is the official fapiao date
  let re = Regex::new(r"开票日期[：:]\s*(\d{4})年(\d{1,2})月(\d{1,2})日")?;
  if let Some(caps) = re.captures(&text)
    && let Some(y) = caps.get(1)
    && let Some(m) = caps.get(2)
    && let Some(d) = caps.get(3)
  {
    let date: Date = Date::new(
      y.as_str().parse()?,
      m.as_str().parse()?,
      d.as_str().parse()?,
    );
    fapiao.date = Some(date);
  } else {
    // Fallback: find any date if 开票日期 not found
    let re = Regex::new(r"(\d{4})年(\d{1,2})月(\d{1,2})日")?;
    if let Some(caps) = re.captures(&text)
      && let Some(y) = caps.get(1)
      && let Some(m) = caps.get(2)
      && let Some(d) = caps.get(3)
    {
      let date: Date = Date::new(
        y.as_str().parse()?,
        m.as_str().parse()?,
        d.as_str().parse()?,
      );
      fapiao.date = Some(date);
    }
  }

  // ── AMOUNT (小写) ──────────────────────────────────────────────────────────
  // Try each strategy in order; first match wins.
  let strategies: [fn(&str) -> Result<Option<String>>; 7] = [
    s1_labeled,
    s2_didi,
    s3_daxie_prefix,
    s4_daxie_suffix,
    s4b_restaurant,
    s5_metro,
    s6_bare_yen_triplet,
  ];
  let mut amount = None;
  for strategy in strategies {
    if amount.is_none() {
      amount = strategy(&text)?;
    }
  }
  if amount.is_none() && is_railway_ticket {
    amount = s7_railway(&text)?;
  }
  if let Some(a) = amount {
    let amount_float = a.parse::<f32>()?;
    if amount_float > 0_f32 && amount_float <= 1000000_f32 {
      fapiao.amount = Some(a);
    }
  }

  // ── VAT AMOUNT ─────────────────────────────────────────────────────────────
  let amt_float = fapiao.amount.as_ref().and_then(|a| a.parse::<f32>().ok());
  let strategies: [fn(&str, Option<f32>) -> Result<Option<String>>; 6] = [
    v1_inline,
    v2_heji_prefix,
    v3_daxie_suffix,
    v4_daxie_suffix,
    v5_bare_yen_triplet,
    v6_bare_yen_fallback,
  ];
  let mut vat_amount = None;
  for strategy in strategies {
    if vat_amount.is_none() {
      vat_amount = strategy(&text, amt_float)?;
    }
  }
  if vat_amount.is_none() && is_railway_ticket {
    vat_amount = v7_railway(&fapiao.amount)?;
  }
  if let Some(a) = vat_amount {
    let vat_amount_float = a.parse::<f32>()?;
    if vat_amount_float > 0_f32 && vat_amount_float <= 1000000_f32 {
      fapiao.vat_amount = Some(a);
    }
  }
  Ok(fapiao)
}

// ── amount strategies ────────────────────────────────────────────────────────

/// Extract group 1 of the first regex match, cleaned of commas/whitespace.
fn capture_amount(re: &Regex, text: &str) -> Option<String> {
  re.captures(text)
    .and_then(|caps| caps.get(1))
    .map(|m| clean(m.as_str()))
}

/// Check the T = P + V invariant for a triplet of capture-group matches.
fn total_matches(
  t: Option<regex::Match>,
  p: Option<regex::Match>,
  v: Option<regex::Match>,
) -> Result<Option<String>> {
  if let (Some(t), Some(p), Some(v)) = (t, p, v) {
    let p: f32 = p.as_str().parse()?;
    let v: f32 = v.as_str().parse()?;
    if approx_eq(t.as_str().parse()?, p + v, None) {
      return Ok(Some(clean(t.as_str())));
    }
  }
  Ok(None)
}

/// S1: Labeled  （小写）¥xxx  ── Walmart, hotels
fn s1_labeled(text: &str) -> Result<Option<String>> {
  let re = Regex::new(&format!(r"[（(]小写[）)]\s*[¥￥]\s*({})", NUM))?;
  Ok(capture_amount(&re, text))
}

/// S2: DiDi/transport  （小写）\nxxx\n¥
fn s2_didi(text: &str) -> Result<Option<String>> {
  let re = Regex::new(&format!(r"[（(]小写[）)]\s*\n\s*({})\s*\n\s*[¥￥]", NUM))?;
  Ok(capture_amount(&re, text))
}

/// S3: Amount precedes 大写  ── Meituan multi-page last page: ¥xxx\n大写
fn s3_daxie_prefix(text: &str) -> Result<Option<String>> {
  let re = Regex::new(&format!(r"[¥￥]({})\n{}", NUM, DAXIE))?;
  Ok(capture_amount(&re, text))
}

/// S4: Amount follows 大写  ── e-commerce, travel: 大写\n¥xxx
fn s4_daxie_suffix(text: &str) -> Result<Option<String>> {
  let re = Regex::new(&format!(r"{}\n[¥￥]({})", DAXIE, NUM))?;
  Ok(capture_amount(&re, text))
}

/// S4b: Restaurant format: 大写\nT\nP\nV where T = P + V.
/// Rust's regex crate has no lookahead, so the Python `(?![¥￥])` guards are
/// approximated with optional/mandatory ¥ prefixes per line.
fn s4b_restaurant(text: &str) -> Result<Option<String>> {
  let re = Regex::new(&format!(
    r"{}\n[¥￥]*({})\n[¥￥]*({})\n[¥￥]*({})",
    DAXIE, NUM, NUM, NUM
  ))?;
  let Some(caps) = re.captures(text) else {
    return Ok(None);
  };
  total_matches(caps.get(1), caps.get(2), caps.get(3))
}

/// S5: Metro/Makro format  ── three bare numbers before buyer name: P\nV\nT\n美国
fn s5_metro(text: &str) -> Result<Option<String>> {
  let re = Regex::new(&format!(
    r"({})\n({})\n({})\n(?:美国|美利坚)",
    NUM, NUM, NUM
  ))?;
  let Some(caps) = re.captures(text) else {
    return Ok(None);
  };
  total_matches(caps.get(3), caps.get(1), caps.get(2))
}

/// S6: Bare-¥ triplet  ── Domino's format: T\n¥  P\n¥  V\n¥ (scattered)
fn s6_bare_yen_triplet(text: &str) -> Result<Option<String>> {
  let re = Regex::new(&format!(r"({})\n[¥￥]", NUM))?;
  let mut bare_vals = re
    .captures_iter(text)
    .map(|c| c.get(1).map_or_else(|| "", |n| n.as_str()))
    .map(|c| clean(c))
    .filter_map(|c| c.parse::<f32>().ok())
    .collect::<Vec<f32>>();

  if bare_vals.len() < 3 {
    return Ok(None);
  }
  let mut bare_set: Vec<f32> = bare_vals
    .iter()
    .map(|x| (x * 100_f32).round() / 100_f32)
    .collect();
  bare_set.sort_unstable_by(f32::total_cmp);
  bare_set.dedup();

  // reverse sort bare_vals
  bare_vals.sort_unstable_by(f32::total_cmp);
  bare_vals.reverse();
  for c in &bare_vals {
    for a in &bare_vals {
      if a == c {
        continue;
      }
      let b = ((c - a) * 100_f32).round() / 100_f32;
      if b > 0_f32 && bare_set.contains(&b) && !approx_eq(b, *c, None) {
        return Ok(Some(format!("{:.2}", c)));
      }
    }
  }
  Ok(None)
}

/// S7: Railway e-tickets ── 票价 followed by ¥xxx (may be on next line)
fn s7_railway(text: &str) -> Result<Option<String>> {
  // Pattern 1: Same line - 票价:¥xxx or 票价：¥xxx
  let re = Regex::new(&format!(r"票价[：:]\s*[¥￥]\s*({})", NUM))?;
  if let Some(amount) = capture_amount(&re, text) {
    return Ok(Some(amount));
  }
  // Pattern 2: ¥xxx within 500 chars after 票价
  let re = Regex::new(&format!(r"票价[：:].{{0,500}}?[¥￥]\s*({})", NUM))?;
  Ok(capture_amount(&re, text))
}

// ── vat amount strategies ────────────────────────────────────────────────────────
/// V1: Labeled  （小写）¥xxx  ── Walmart, hotels
fn v1_inline(text: &str, _amt: Option<f32>) -> Result<Option<String>> {
  let re = Regex::new(&format!(r"合\s+计\s+[¥￥]{}\s+[¥￥]({})", NUM, NUM))?;
  Ok(capture_amount(&re, text))
}

// V2: DiDi format  ── 合\n计\nP\n¥\nV\n¥
fn v2_heji_prefix(text: &str, _amt: Option<f32>) -> Result<Option<String>> {
  let re = Regex::new(&format!(r"合\n计\n{}\n[¥￥]\n({})\n[¥￥]", NUM, NUM))?;
  Ok(capture_amount(&re, text))
}

// V3: E-commerce  ── ¥P\n¥V\n大写  (skip if captured value == total)
fn v3_daxie_suffix(text: &str, amt: Option<f32>) -> Result<Option<String>> {
  let re = Regex::new(&format!(r"[¥￥]{}\n[¥￥]({})\n{}", NUM, NUM, DAXIE))?;
  let Some(candidate) = capture_amount(&re, text) else {
    return Ok(None);
  };
  match amt {
    Some(a) if approx_eq(candidate.parse()?, a, None) => Ok(None),
    _ => Ok(Some(candidate)),
  }
}

// V4: Restaurant format  ── 大写\nT\nP\nV (third number = VAT)
fn v4_daxie_suffix(text: &str, _amt: Option<f32>) -> Result<Option<String>> {
  let re = Regex::new(&format!(
    r"{}\n[¥￥]*({})\n[¥￥]*({})\n[¥￥]*({})",
    DAXIE, NUM, NUM, NUM
  ))?;
  let Some(caps) = re.captures(text) else {
    return Ok(None);
  };
  if total_matches(caps.get(1), caps.get(2), caps.get(3))?.is_some() {
    return Ok(caps.get(3).map(|v| clean(v.as_str())));
  }
  Ok(None)
}

// V5: Metro/Makro format  ── P\nV\nT\n美国 (second number = VAT)
fn v5_bare_yen_triplet(text: &str, _amt: Option<f32>) -> Result<Option<String>> {
  let re = Regex::new(&format!(
    r"({})\n({})\n({})\n(?:美国|美利坚)",
    NUM, NUM, NUM
  ))?;
  let Some(caps) = re.captures(text) else {
    return Ok(None);
  };
  if total_matches(caps.get(3), caps.get(1), caps.get(2))?.is_some() {
    return Ok(caps.get(2).map(|v| clean(v.as_str())));
  }
  Ok(None)
}

// V6: Fallback  ── find ¥ or bare-¥ value that pairs with another to equal total
fn v6_bare_yen_fallback(text: &str, amt: Option<f32>) -> Result<Option<String>> {
  let Some(amt_float) = amt else {
    return Ok(None);
  };
  let re_yen = Regex::new(&format!(r"[¥￥]({})", NUM))?;
  let re_bare = Regex::new(&format!(r"({})\n[¥￥]", NUM))?;
  let parse_vals = |re: &Regex| -> Vec<f32> {
    re.captures_iter(text)
      .filter_map(|c| c.get(1))
      .map(|m| clean(m.as_str()))
      .filter_map(|c| c.parse::<f32>().ok())
      .collect()
  };
  let mut all_vals: Vec<f32> = parse_vals(&re_yen);
  all_vals.extend(parse_vals(&re_bare));

  let mut val_set: Vec<f32> = all_vals
    .iter()
    .map(|x| (x * 100_f32).round() / 100_f32)
    .collect();
  val_set.sort_unstable_by(f32::total_cmp);
  val_set.dedup();

  for a in &all_vals {
    if approx_eq(*a, amt_float, None) {
      continue;
    }
    let b = ((amt_float - a) * 100_f32).round() / 100_f32;
    if b > 0_f32 && val_set.contains(&b) && !approx_eq(b, amt_float, None) {
      return Ok(Some(format!("{:.2}", a.min(b))));
    }
  }
  Ok(None)
}

fn v7_railway(amount: &Option<String>) -> Result<Option<String>> {
  if let Some(a) = amount {
    let amount_float: f32 = a.parse()?;
    return Ok(Some(format!("{:.2}", amount_float * 3.0 / 103_f32)));
  }
  Ok(None)
}

/// Convert full-width characters to ASCII equivalents.
fn normalize_fullwidth(text: &str) -> String {
  text
    .chars()
    .map(|c| match c {
      '０'..='９' => shift(c, '０', '0'),
      'Ａ'..='Ｚ' => shift(c, 'Ａ', 'A'),
      'ａ'..='ｚ' => shift(c, 'ａ', 'a'),
      '．' => '.',
      '，' => ',',
      '：' => ':',
      '；' => ';',
      '（' => '(',
      '）' => ')',
      '￥' => '¥',
      _ => c,
    })
    .collect()
}

fn shift(c: char, from: char, to: char) -> char {
  char::from_u32(c as u32 - from as u32 + to as u32).unwrap_or(c)
}

fn clean(str: &str) -> String {
  str.replace(",", "").trim().to_string()
}

fn approx_eq(a: f32, b: f32, tol: Option<f32>) -> bool {
  let tol_num = tol.unwrap_or_else(|| 0.02);
  return (a - b).abs() <= tol_num;
}

#[derive(Debug)]
struct Fapiao {
  fapiao_number: Option<String>,
  date: Option<Date>,
  amount: Option<String>,
  vat_amount: Option<String>,
  products: Option<Vec<String>>,
  skip: bool,
  skip_reason: Option<String>,
}

#[cfg(test)]
mod tests {
  use simple_datetime_rs::Format;

  use super::*;

  // ── helpers ────────────────────────────────────────────────────────────────

  fn parse(text: &str) -> Result<Fapiao> {
    parse_fapiao(text.to_string())
  }

  // test helpers
  fn test_clean_strips_commas_and_whitespace() -> Result<()> {
    assert_eq!(clean("1,234.56"), "1234.56".to_string());
    assert_eq!(clean("  99 "), "99".to_string());
    assert_eq!(clean("1,000,000.00"), "1000000.00".to_string());
    Ok(())
  }

  /// "YYYY-MM-DD" string so dates can be compared like the Python tests do.
  fn date_str(f: &Fapiao) -> Option<String> {
    f.date.as_ref().map(|d| d.format("%Y-%m-%d").unwrap())
  }

  #[test]
  fn extracts_from_sample_pdf() -> Result<()> {
    let bytes = include_bytes!("../fixtures/combined_fapiaos.pdf");
    let out = extract(bytes)?;
    println!("{out}");
    Ok(())
  }

  // ── garbled / skip detection ───────────────────────────────────────────────

  #[test]
  fn test_skip_garbled_no_nian() -> Result<()> {
    // Pages without 年 are garbled (airline/train ticket PDFs).
    let result = parse("Invoice 12345 amount 100")?;
    assert!(result.skip);
    assert_eq!(result.skip_reason.as_deref(), Some("garbled text"));
    Ok(())
  }

  #[test]
  fn test_skip_continuation_page() -> Result<()> {
    let result =
      parse("发票号码：012345678901234\n2024年3月15日\n共 3 页 第 2 页\n名称：测试公司有限公司")?;
    assert!(result.skip);
    assert!(result.skip_reason.unwrap().contains("2"));
    Ok(())
  }

  #[test]
  fn test_last_page_not_skipped() -> Result<()> {
    // 共 N 页 第 N 页 means last page — should NOT be skipped.
    let result =
      parse("发票号码：012345678901234\n2024年3月15日\n共 2 页 第 2 页\n名称：测试公司有限公司")?;
    assert!(!result.skip);
    Ok(())
  }

  // ── fapiao number ──────────────────────────────────────────────────────────

  #[test]
  fn test_fapiao_number_labeled() -> Result<()> {
    let result = parse("年\n发票号码：012345678901234\n2024年3月5日")?;
    assert_eq!(result.fapiao_number.as_deref(), Some("012345678901234"));
    Ok(())
  }

  #[test]
  fn test_fapiao_number_20digit_fallback() -> Result<()> {
    let result = parse("年\n2024年1月1日\n00000000000000000001\n金额")?;
    assert_eq!(
      result.fapiao_number.as_deref(),
      Some("00000000000000000001")
    );
    Ok(())
  }

  #[test]
  fn test_fapiao_number_none_when_absent() -> Result<()> {
    let result = parse("年\n2024年1月1日\n金额")?;
    assert!(result.fapiao_number.is_none());
    Ok(())
  }

  // ── date parsing ───────────────────────────────────────────────────────────

  #[test]
  fn test_date_parsed_correctly() -> Result<()> {
    let result = parse("年\n2024年3月5日")?;
    assert_eq!(date_str(&result).as_deref(), Some("2024-03-05"));
    Ok(())
  }

  #[test]
  fn test_date_two_digit_day_month() -> Result<()> {
    let result = parse("年\n2023年12月31日")?;
    assert_eq!(date_str(&result).as_deref(), Some("2023-12-31"));
    Ok(())
  }

  #[test]
  fn test_date_none_when_absent() -> Result<()> {
    let result = parse("年\n金额100元")?;
    assert!(result.date.is_none());
    Ok(())
  }

  // ── 开票日期 preference ─────────────────────────────────────────────────────

  #[test]
  fn test_date_prefers_kaijiao_riqi_when_present() -> Result<()> {
    // When 开票日期 is present, it wins over other dates.
    let result = parse(
      "发票号码：012345678901234\n2024年3月10日\n开票日期：2024年3月15日\n（小写）¥188.50\n名称：测试公司有限公司\n年",
    )?;
    assert_eq!(date_str(&result).as_deref(), Some("2024-03-15"));
    Ok(())
  }

  #[test]
  fn test_date_prefers_kaijiao_riqi_with_colon_variant() -> Result<()> {
    let result = parse(
      "发票号码：012345678901234\n2024年3月10日\n开票日期:2024年3月20日\n（小写）¥188.50\n名称：测试公司有限公司\n年",
    )?;
    assert_eq!(date_str(&result).as_deref(), Some("2024-03-20"));
    Ok(())
  }

  #[test]
  fn test_date_kaijiao_riqi_with_fullwidth_colon() -> Result<()> {
    let result = parse(
      "发票号码：012345678901234\n2024年3月10日\n开票日期：2024年3月25日\n（小写）¥188.50\n名称：测试公司有限公司\n年",
    )?;
    assert_eq!(date_str(&result).as_deref(), Some("2024-03-25"));
    Ok(())
  }

  #[test]
  fn test_date_kaijiao_riqi_with_spaces() -> Result<()> {
    let result = parse(
      "发票号码：012345678901234\n2024年3月10日\n开票日期: 2024年3月30日\n（小写）¥188.50\n名称：测试公司有限公司\n年",
    )?;
    assert_eq!(date_str(&result).as_deref(), Some("2024-03-30"));
    Ok(())
  }

  #[test]
  fn test_date_fallback_when_no_kaijiao_riqi() -> Result<()> {
    // When 开票日期 is absent, use the first date found.
    let result = parse(
      "发票号码：012345678901234\n2024年3月15日\n（小写）¥188.50\n名称：测试公司有限公司\n年",
    )?;
    assert_eq!(date_str(&result).as_deref(), Some("2024-03-15"));
    Ok(())
  }

  #[test]
  fn test_date_prefers_kaijiao_riqi_in_railway_ticket() -> Result<()> {
    // Railway tickets: prefer 开票日期 over ride date.
    let result = parse(
      "发票号码:26429165848005761994\n广州南站\n2026年06月19日\n电子发票（铁路电子客票）\n07:25开\n票价:￥203.00\n开票日期:2026年06月21日\n买票请到12306 发货请到95306\n中国铁路祝您旅途愉快\n",
    )?;
    assert_eq!(date_str(&result).as_deref(), Some("2026-06-21"));
    Ok(())
  }

  // ── amount strategies ──────────────────────────────────────────────────────

  #[test]
  fn test_s1_walmart_style() -> Result<()> {
    // S1: （小写）¥xxx
    let result =
      s1_labeled("年\n2024年1月1日\n（小写）¥188.50\n名称：沃尔玛（湖北）商业零售有限公司")?;
    assert_eq!(result, Some("188.50".to_string()));
    Ok(())
  }

  #[test]
  fn test_s1_fullwidth_yen() -> Result<()> {
    let result = s1_labeled("年\n2024年1月1日\n（小写）￥99.00")?;
    assert_eq!(result, Some("99.00".to_string()));
    Ok(())
  }

  #[test]
  fn test_s1_comma_in_amount() -> Result<()> {
    let result = s1_labeled("年\n2024年1月1日\n（小写）¥1,234.56")?;
    assert_eq!(result, Some("1234.56".to_string()));
    Ok(())
  }

  #[test]
  fn test_s2_didi_style() -> Result<()> {
    // S2: （小写）\nxxx\n¥
    let result = s2_didi("年\n2024年1月1日\n（小写）\n45.60\n¥")?;
    assert_eq!(result, Some("45.60".to_string()));
    Ok(())
  }

  #[test]
  fn test_s3_meituan_prefix() -> Result<()> {
    // S3: ¥xxx\n大写 — amount precedes 大写
    let result = s3_daxie_prefix("年\n2024年1月1日\n¥55.00\n壹拾贰圆整")?;
    assert_eq!(result, Some("55.00".to_string()));
    Ok(())
  }

  #[test]
  fn test_s4_ecommerce_amount_follows_daxie() -> Result<()> {
    // S4: 大写\n¥xxx
    let result = s4_daxie_suffix("年\n2024年1月1日\n壹佰贰拾叁圆整\n¥123.00")?;
    assert_eq!(result, Some("123.00".to_string()));
    Ok(())
  }

  #[test]
  fn test_s4b_restaurant_t_p_v_triplet() -> Result<()> {
    // S4b: 大写\nT\nP\nV where T = P + V
    let result = s4b_restaurant("年\n2024年1月1日\n壹佰圆整\n100.00\n94.34\n5.66\n")?;
    assert_eq!(result, Some("100.00".to_string()));
    Ok(())
  }

  #[test]
  fn test_s5_metro_format() -> Result<()> {
    // S5: P\nV\nT\n美国 where T = P + V
    let result = s5_metro("年\n2024年1月1日\n94.34\n5.66\n100.00\n美国驻武汉总领事馆\n")?;
    assert_eq!(result, Some("100.00".to_string()));
    Ok(())
  }

  #[test]
  fn test_s6_dominos_bare_yen_triplet() -> Result<()> {
    // S6: T\n¥  P\n¥  V\n¥ scattered in text
    let result = parse("年\n2024年1月1日\n100.00\n¥\n94.34\n¥\n5.66\n¥\n")?;
    assert_eq!(result.amount, Some("100.00".to_string()));
    Ok(())
  }

  #[test]
  fn test_amount_none_when_not_found() -> Result<()> {
    let result = parse("年\n2024年1月1日\n名称：测试公司有限公司")?;
    assert!(result.amount.is_none());
    Ok(())
  }

  // ── VAT strategies ─────────────────────────────────────────────────────────

  #[test]
  fn test_v1_walmart_he_ji() -> Result<()> {
    // V1: 合   计 ¥P ¥V
    let result = parse("年\n2024年1月1日\n（小写）¥188.50\n合     计  ¥176.17  ¥12.33")?;
    assert_eq!(result.vat_amount, Some("12.33".to_string()));
    Ok(())
  }

  #[test]
  fn test_v2_didi_he_ji() -> Result<()> {
    // V2: 合\n计\nP\n¥\nV\n¥
    let result = parse("年\n2024年1月1日\n（小写）\n45.60\n¥\n合\n计\n42.62\n¥\n2.98\n¥")?;
    assert_eq!(result.vat_amount, Some("2.98".to_string()));
    Ok(())
  }

  #[test]
  fn test_v4_restaurant_vat() -> Result<()> {
    // V4: 大写\nT\nP\nV (third = VAT)
    let result = parse("年\n2024年1月1日\n壹佰圆整\n100.00\n94.34\n5.66\n")?;
    assert_eq!(result.vat_amount, Some("5.66".to_string()));
    Ok(())
  }

  #[test]
  fn test_v5_metro_vat() -> Result<()> {
    // V5: P\nV\nT\n美国 (second = VAT)
    let result = parse("年\n2024年1月1日\n94.34\n5.66\n100.00\n美国驻武汉总领事馆\n")?;
    assert_eq!(result.vat_amount, Some("5.66".to_string()));
    Ok(())
  }

  #[test]
  fn test_vat_none_when_not_found() -> Result<()> {
    let result = parse("年\n2024年1月1日\n（小写）¥50.00")?;
    assert!(result.vat_amount.is_none());
    Ok(())
  }

  // ── full parse integration ─────────────────────────────────────────────────

  #[test]
  fn test_full_parse_walmart_style() -> Result<()> {
    let result = parse(
      "发票号码：012345678901234\n2024年3月15日\n（小写）¥188.50\n合     计  ¥176.17  ¥12.33\n名称：沃尔玛（湖北）商业零售有限公司\n年\n",
    )?;
    assert!(!result.skip);
    assert_eq!(result.fapiao_number.as_deref(), Some("012345678901234"));
    assert_eq!(date_str(&result).as_deref(), Some("2024-03-15"));
    assert_eq!(result.amount, Some("188.50".to_string()));
    assert_eq!(result.vat_amount, Some("12.33".to_string()));
    // NOTE: Python also asserts seller == '沃尔玛(湖北)商业零售有限公司'
    // — port once Fapiao has a seller field.
    Ok(())
  }

  #[test]
  fn test_full_parse_metro_style() -> Result<()> {
    let result = parse(
      "发票号码：012345678901235\n2024年6月1日\n94.34\n5.66\n100.00\n美国驻武汉总领事馆\n上海麦德龙商贸有限公司武汉分公司\n年\n",
    )?;
    assert!(!result.skip);
    assert_eq!(result.amount, Some("100.00".to_string()));
    assert_eq!(result.vat_amount, Some("5.66".to_string()));
    // NOTE: Python also asserts '麦德龙' in seller.
    Ok(())
  }

  // ── railway e-ticket parsing ───────────────────────────────────────────────

  #[test]
  fn test_railway_ticket_fullwidth_parsing() -> Result<()> {
    let result = parse(
      "发票号码：２６４４９１２４０８８０００２０８４３８\n开票日期：２０２６年０６月２２日\n电子发票（铁路电子客票）\n票价：￥１３４．００\n买票请到12306 发货请到95306\n中国铁路祝您旅途愉快\n",
    )?;
    assert!(!result.skip);
    assert_eq!(
      result.fapiao_number.as_deref(),
      Some("26449124088000208438")
    );
    assert_eq!(date_str(&result).as_deref(), Some("2026-06-22"));
    assert_eq!(result.amount, Some("134.00".to_string()));
    // NOTE: Python also asserts seller == '中国铁路'.
    Ok(())
  }

  #[test]
  fn test_railway_ticket_vat_calculation() -> Result<()> {
    // Railway ticket VAT is calculated at 3%: 103 * 3 / 103 = 3.0
    let result = parse(
      "发票号码：12345678901234567890\n2026年06月22日\n电子发票（铁路电子客票）\n票价：¥103.00\n买票请到12306\n中国铁路祝您旅途愉快\n",
    )?;
    assert!(!result.skip);
    assert_eq!(result.vat_amount, Some("3.00".to_string()));
    Ok(())
  }

  #[test]
  fn test_railway_ticket_vat_calculation_rounding() -> Result<()> {
    // VAT = 134 * 3 / 103 = 3.9029... ≈ 3.90
    let result = parse(
      "2026年06月22日\n电子发票（铁路电子客票）\n票价：¥134.00\n买票请到12306\n中国铁路祝您旅途愉快\n",
    )?;
    assert_eq!(result.vat_amount, Some("3.90".to_string()));
    Ok(())
  }

  #[test]
  fn test_regular_fapiao_not_affected_by_railway_logic() -> Result<()> {
    // Regular fapiaos use extracted VAT, not the railway calculation.
    let result = parse(
      "发票号码：012345678901234\n2024年3月15日\n（小写）¥188.50\n合     计  ¥176.17  ¥12.33\n名称：沃尔玛（湖北）商业零售有限公司\n年\n",
    )?;
    assert!(!result.skip);
    assert_eq!(result.vat_amount, Some("12.33".to_string()));
    Ok(())
  }
}
