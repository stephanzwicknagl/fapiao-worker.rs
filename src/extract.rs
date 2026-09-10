use pdf_oxide::PdfDocument;
use regex::Regex;
use simple_datetime_rs::Date;

use crate::Result;
use crate::model::Fapiao;

const NUM: &str = r"[\d,]+\.?\d*";
const DAXIE: &str =
  r"[壹贰叁肆伍陆柒捌玖拾零百千万亿佰仟][壹贰叁肆伍陆柒捌玖拾零百千万亿佰仟圆元角分整]{2,}[整]?";

pub fn extract(bytes_vec: Vec<Vec<u8>>) -> Result<Vec<Fapiao>> {
  let mut fapiaos: Vec<Fapiao> = vec![];
  for bytes in bytes_vec {
    let doc = PdfDocument::from_bytes(bytes)?;
    let pages_total = doc.page_count()?;
    for i in 0..pages_total {
      let text = doc.extract_text_auto(i)?;
      let new = parse_fapiao(text.clone())?;
      fapiaos.push(new);
    }
  }
  sort_fapiaos(&mut fapiaos);
  Ok(fapiaos)
}

fn sort_fapiaos(fapiaos: &mut Vec<Fapiao>) {
  fapiaos.sort_unstable_by(|f1, f2| {
    f1.date.cmp(&f2.date).then_with(|| {
      f2.amount
        .partial_cmp(&f1.amount)
        .unwrap_or_else(|| core::cmp::Ordering::Less)
    })
  });
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
      seller: None,
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
    && total.as_str().parse::<i32>()? > current.as_str().parse::<i32>()?
  {
    return Ok(Fapiao {
      fapiao_number: None,
      date: None,
      amount: None,
      vat_amount: None,
      seller: None,
      products: None,
      skip: true,
      skip_reason: Some(format!("page {} of {}", current.as_str(), total.as_str())),
    });
  }
  let mut fapiao = Fapiao {
    fapiao_number: None,
    date: None,
    amount: None,
    vat_amount: None,
    seller: None,
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
  let strategies: [AmountStrategy; 11] = [
    s1_labeled,
    s1b_mixed_label,
    s2_didi,
    s3_daxie_prefix,
    s4_daxie_suffix,
    s4b_restaurant,
    s4c_daxie_inline,
    s4d_daxie_bare_inline,
    s5_metro,
    s6_bare_yen_triplet,
    s8_heji_sum,
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
    if a > 0_f32 && a <= 1000000_f32 {
      fapiao.amount = Some(rounded_for_cents(a));
    }
  }

  // ── VAT AMOUNT ─────────────────────────────────────────────────────────────
  let strategies: [VatStrategy; 8] = [
    v1_inline,
    v1b_heji_suffix,
    v2_heji_prefix,
    v3_daxie_suffix,
    v4_daxie_suffix,
    v5_bare_yen_triplet,
    v5b_metro_inline,
    v6_bare_yen_fallback,
  ];
  let mut vat_amount = None;
  for strategy in strategies {
    if vat_amount.is_none() {
      vat_amount = strategy(&text, fapiao.amount)?;
    }
  }
  if vat_amount.is_none() && is_railway_ticket {
    vat_amount = v7_railway(fapiao.amount)?;
  }
  if let Some(a) = vat_amount {
    if a > 0_f32 && a <= 1000000_f32 {
      fapiao.vat_amount = Some(rounded_for_cents(a));
    }
  }

  // ── SELLER ─────────────────────────────────────────────────────────────────
  fapiao.seller = extract_seller(&text)?;

  // ── PRODUCTS ───────────────────────────────────────────────────────────────
  fapiao.products = Some(extract_products(&text)?);

  Ok(fapiao)
}

type AmountStrategy = fn(&str) -> Result<Option<f32>>;
type VatStrategy = fn(&str, Option<f32>) -> Result<Option<f32>>;

// ── amount strategies ────────────────────────────────────────────────────────

/// Extract group 1 of the first regex match, cleaned of commas/whitespace.
fn capture_amount(re: &Regex, text: &str) -> Option<f32> {
  re.captures(text)
    .and_then(|caps| caps.get(1))
    .and_then(|m| clean(m.as_str()).parse::<f32>().ok())
}

/// Check the T = P + V invariant for a triplet of capture-group matches.
fn total_matches(
  t: Option<regex::Match>,
  p: Option<regex::Match>,
  v: Option<regex::Match>,
) -> Result<Option<f32>> {
  if let (Some(t), Some(p), Some(v)) = (t, p, v) {
    let p: f32 = p.as_str().parse()?;
    let v: f32 = v.as_str().parse()?;
    if approx_eq(t.as_str().parse()?, p + v, None) {
      return Ok(clean(t.as_str()).parse().ok());
    }
  }
  Ok(None)
}

fn rounded_for_cents(x: f32) -> f32 {
  (x * 100_f32).round() / 100_f32
}

/// S1: Labeled  （小写）¥xxx  ── Walmart, hotels.
/// Also matches labels spaced out character-by-character: （ 小 写 ） ¥xxx (DiDi).
fn s1_labeled(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(r"[（(]\s*小\s*写\s*[）)]\s*[¥￥]\s*({})", NUM))?;
  Ok(capture_amount(&re, text))
}

/// S1b: Jumbled one-line label ── （小写）价税合计（大写） ¥xxx (Wagas restaurant)
fn s1b_mixed_label(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(
    r"[（(]\s*小\s*写\s*[）)][^¥￥\n]{{0,30}}[¥￥]\s*({})",
    NUM
  ))?;
  Ok(capture_amount(&re, text))
}

/// S2: DiDi/transport  （小写）\nxxx\n¥
fn s2_didi(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(r"[（(]小写[）)]\s*\n\s*({})\s*\n\s*[¥￥]", NUM))?;
  Ok(capture_amount(&re, text))
}

/// S3: Amount precedes 大写  ── Meituan multi-page last page: ¥xxx\n大写
fn s3_daxie_prefix(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(r"[¥￥]({})\n{}", NUM, DAXIE))?;
  let re_inline = Regex::new(&format!(r"^[ \t]*[¥￥]{}", NUM))?;
  for caps in re.captures_iter(text) {
    let Some(m) = caps.get(0) else { continue };
    // Skip when the 大写 line continues with an inline ¥amount
    // (壹佰圆整 ¥100.00) — the ¥xxx before it is then a 合计 line value
    // (price or VAT), and the inline total is handled by s4c_daxie_inline.
    if re_inline.is_match(&text[m.end()..]) {
      continue;
    }
    // Skip when the ¥xxx is the second value of a "¥P ¥V" pair on its line
    // (a 合计 line) — it is then the VAT, not the total.
    let line_start = text[..m.start()].rfind('\n').map_or(0, |i| i + 1);
    if text[line_start..m.start()].contains(['¥', '￥']) {
      continue;
    }
    if let Some(a) = caps
      .get(1)
      .and_then(|n| clean(n.as_str()).parse::<f32>().ok())
    {
      return Ok(Some(a));
    }
  }
  Ok(None)
}

/// S4: Amount follows 大写  ── e-commerce, travel: 大写\n¥xxx
fn s4_daxie_suffix(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(r"{}\n[¥￥]({})", DAXIE, NUM))?;
  Ok(capture_amount(&re, text))
}

/// S4b: Restaurant format: 大写\nT\nP\nV where T = P + V.
fn s4b_restaurant(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(
    r"{}\n[¥￥]*({})\n[¥￥]*({})\n[¥￥]*({})",
    DAXIE, NUM, NUM, NUM
  ))?;
  let Some(caps) = re.captures(text) else {
    return Ok(None);
  };
  total_matches(caps.get(1), caps.get(2), caps.get(3))
}

/// S4c: 大写 and ¥amount on the same line ── 伍佰肆拾贰圆整 ¥542.00
fn s4c_daxie_inline(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(r"{}[ \t]*[¥￥]({})", DAXIE, NUM))?;
  Ok(capture_amount(&re, text))
}

/// S4d: Metro inline ── P V\n大写T with no ¥ anywhere, where T = P + V.
fn s4d_daxie_bare_inline(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(
    r"({})[ \t]+({})[ \t]*\n[ \t]*{}({})",
    NUM, NUM, DAXIE, NUM
  ))?;
  for caps in re.captures_iter(text) {
    if let Some(t) = total_matches(caps.get(3), caps.get(1), caps.get(2))? {
      return Ok(Some(t));
    }
  }
  Ok(None)
}

/// S5: Metro/Makro format  ── three bare numbers (P, V, T with T = P + V)
/// followed by a non-numeric line (the buyer/seller name block).
fn s5_metro(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(
    r"({})[ \t]*\n[ \t]*({})[ \t]*\n[ \t]*({})[ \t]*\n[ \t]*[^\d\n]",
    NUM, NUM, NUM
  ))?;
  for caps in re.captures_iter(text) {
    if let Some(t) = total_matches(caps.get(3), caps.get(1), caps.get(2))? {
      return Ok(Some(t));
    }
  }
  Ok(None)
}

/// S6: Bare-¥ triplet  ── Domino's format: T\n¥  P\n¥  V\n¥ (scattered)
fn s6_bare_yen_triplet(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(r"({})\n[¥￥]", NUM))?;
  let mut bare_vals = re
    .captures_iter(text)
    .map(|c| c.get(1).map_or_else(|| "", |n| n.as_str()))
    .map(clean)
    .filter_map(|c| c.parse::<f32>().ok())
    .collect::<Vec<f32>>();

  if bare_vals.len() < 3 {
    return Ok(None);
  }
  let mut bare_set: Vec<f32> = bare_vals.iter().map(|x| rounded_for_cents(*x)).collect();
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
      let b = rounded_for_cents(c - a);
      if b > 0_f32 && bare_set.contains(&b) && !approx_eq(b, *c, None) {
        return Ok(Some(*c));
      }
    }
  }
  Ok(None)
}

/// S8: 合计 line with scattered total ── 合 计 ¥P ¥V where the 价税合计
/// total ¥T (T = P + V) floats elsewhere in the extracted text.
fn s8_heji_sum(text: &str) -> Result<Option<f32>> {
  let re = Regex::new(&format!(r"合\s+计\s+[¥￥]({})\s+[¥￥]({})", NUM, NUM))?;
  let Some(caps) = re.captures(text) else {
    return Ok(None);
  };
  let (Some(p), Some(v)) = (caps.get(1), caps.get(2)) else {
    return Ok(None);
  };
  let p: f32 = clean(p.as_str()).parse()?;
  let v: f32 = clean(v.as_str()).parse()?;
  let t = rounded_for_cents(p + v);
  // Confirm a ¥amount equal to the sum exists somewhere in the text.
  let re_yen = Regex::new(&format!(r"[¥￥]({})", NUM))?;
  for caps in re_yen.captures_iter(text) {
    if let Some(m) = caps.get(1)
      && let Ok(a) = clean(m.as_str()).parse::<f32>()
      && approx_eq(a, t, None)
    {
      return Ok(Some(t));
    }
  }
  Ok(None)
}

/// S7: Railway e-tickets ── 票价 followed by ¥xxx (may be on next line)
fn s7_railway(text: &str) -> Result<Option<f32>> {
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
fn v1_inline(text: &str, _amt: Option<f32>) -> Result<Option<f32>> {
  let re = Regex::new(&format!(r"合\s+计\s+[¥￥]{}\s+[¥￥]({})", NUM, NUM))?;
  Ok(capture_amount(&re, text))
}

// V1b: 合计 label after the amounts ── ¥P ¥V\n合 计  (second ¥ value = VAT)
fn v1b_heji_suffix(text: &str, _amt: Option<f32>) -> Result<Option<f32>> {
  let re = Regex::new(&format!(
    r"[¥￥]{}[ \t]+[¥￥]({})[ \t]*\n[ \t]*合\s*计",
    NUM, NUM
  ))?;
  Ok(capture_amount(&re, text))
}

// V2: DiDi format  ── 合\n计\nP\n¥\nV\n¥
fn v2_heji_prefix(text: &str, _amt: Option<f32>) -> Result<Option<f32>> {
  let re = Regex::new(&format!(r"合\n计\n{}\n[¥￥]\n({})\n[¥￥]", NUM, NUM))?;
  Ok(capture_amount(&re, text))
}

// V3: E-commerce  ── ¥P\n¥V\n大写  (skip if captured value == total)
fn v3_daxie_suffix(text: &str, amt: Option<f32>) -> Result<Option<f32>> {
  let re = Regex::new(&format!(r"[¥￥]{}\n[¥￥]({})\n{}", NUM, NUM, DAXIE))?;
  let Some(candidate) = capture_amount(&re, text) else {
    return Ok(None);
  };
  match amt {
    Some(a) if approx_eq(candidate, a, None) => Ok(None),
    _ => Ok(Some(candidate)),
  }
}

// V4: Restaurant format  ── 大写\nT\nP\nV (third number = VAT)
fn v4_daxie_suffix(text: &str, _amt: Option<f32>) -> Result<Option<f32>> {
  let re = Regex::new(&format!(
    r"{}\n[¥￥]*({})\n[¥￥]*({})\n[¥￥]*({})",
    DAXIE, NUM, NUM, NUM
  ))?;
  let Some(caps) = re.captures(text) else {
    return Ok(None);
  };
  if total_matches(caps.get(1), caps.get(2), caps.get(3))?.is_some() {
    return Ok(caps.get(3).and_then(|v| clean(v.as_str()).parse().ok()));
  }
  Ok(None)
}

// V5: Metro/Makro format  ── P\nV\nT + non-numeric line (second number = VAT)
fn v5_bare_yen_triplet(text: &str, _amt: Option<f32>) -> Result<Option<f32>> {
  let re = Regex::new(&format!(
    r"({})[ \t]*\n[ \t]*({})[ \t]*\n[ \t]*({})[ \t]*\n[ \t]*[^\d\n]",
    NUM, NUM, NUM
  ))?;
  for caps in re.captures_iter(text) {
    if total_matches(caps.get(3), caps.get(1), caps.get(2))?.is_some() {
      return Ok(caps.get(2).and_then(|v| clean(v.as_str()).parse().ok()));
    }
  }
  Ok(None)
}

// V5b: Metro inline  ── P V\n大写T (T = P + V), second number = VAT
fn v5b_metro_inline(text: &str, _amt: Option<f32>) -> Result<Option<f32>> {
  let re = Regex::new(&format!(
    r"({})[ \t]+({})[ \t]*\n[ \t]*{}({})",
    NUM, NUM, DAXIE, NUM
  ))?;
  for caps in re.captures_iter(text) {
    if total_matches(caps.get(3), caps.get(1), caps.get(2))?.is_some() {
      return Ok(caps.get(2).and_then(|v| clean(v.as_str()).parse().ok()));
    }
  }
  Ok(None)
}

// V6: Fallback  ── find ¥ or bare-¥ value that pairs with another to equal total
fn v6_bare_yen_fallback(text: &str, amt: Option<f32>) -> Result<Option<f32>> {
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

  let mut val_set: Vec<f32> = all_vals.iter().map(|x| rounded_for_cents(*x)).collect();
  val_set.sort_unstable_by(f32::total_cmp);
  val_set.dedup();

  for a in &all_vals {
    if approx_eq(*a, amt_float, None) {
      continue;
    }
    let b = rounded_for_cents(amt_float - a);
    if b > 0_f32 && val_set.contains(&b) && !approx_eq(b, amt_float, None) {
      return Ok(Some(a.min(b)));
    }
  }
  Ok(None)
}

fn v7_railway(amount: Option<f32>) -> Result<Option<f32>> {
  if let Some(amount) = amount {
    return Ok(Some(amount * 3.0 / 103_f32));
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

/// Identify the buyer structurally: the first 名称 following the 购买方
/// (buyer info) section marker, which fapiaos often print vertically as
/// 购\n买\n方\n信\n息.
fn extract_buyer(text: &str) -> Result<Option<String>> {
  let re = Regex::new(r"购\s*买\s*方[\s\S]*?名称[：:][ \t]*([^\n]+)")?;
  if let Some(name) = re
    .captures(text)
    .and_then(|c| c.get(1))
    .map(|m| cut_at_next_label(m.as_str()))
  {
    return Ok(Some(name));
  }
  // Interleaved 购/销 column layout: the 购 and 销 header characters appear
  // together (side by side or one per line), so the vertical 购买方 marker
  // above cannot match. The first 名称 after the header is the buyer (the
  // second is the seller).
  let re = Regex::new(r"购\s+销[\s\S]{0,50}?名称[：:][ \t]*([^\n]+)")?;
  if let Some(name) = re
    .captures(text)
    .and_then(|c| c.get(1))
    .map(|m| cut_at_next_label(m.as_str()))
  {
    return Ok(Some(name));
  }
  // Fully inline layout: 购 名称：<buyer> 销 名称：<seller> on one line, so
  // the buyer name directly follows the 购 label character.
  let re = Regex::new(r"购[ \t]+名称[：:][ \t]*([^\n]+)")?;
  Ok(
    re.captures(text)
      .and_then(|c| c.get(1))
      .map(|m| cut_at_next_label(m.as_str())),
  )
}

/// Trim whitespace and cut a name at a following 名称 label on the same line
/// (buyer and seller names are sometimes printed side by side on one line).
fn cut_at_next_label(name: &str) -> String {
  let name = match name.find("名称") {
    Some(pos) => &name[..pos],
    None => name,
  };
  let name = name.trim();
  // A name cut at the next 名称 label may keep a trailing interleaved
  // column-label character, e.g. "…公司 销 名称：…" → "…公司 销".
  if let Some(last) = name.chars().last()
    && ['购', '买', '方', '销', '售', '信', '息'].contains(&last)
  {
    let rest = &name[..name.len() - last.len_utf8()];
    if rest.ends_with(char::is_whitespace) {
      return rest.trim_end().to_string();
    }
  }
  name.to_string()
}

/// Extract the seller name from fapiao text.
fn extract_seller(text: &str) -> Result<Option<String>> {
  let buyer = extract_buyer(text)?;

  // Pattern 1: explicit 名称：<seller> (DiDi-style fapiaos). Buyer and seller
  // names may share one line, so each name runs to the next 名称 label or to
  // the end of the line.
  let re = Regex::new(r"名称[：:][ \t]*")?;
  for m in re.find_iter(text) {
    let rest = &text[m.end()..];
    let line = match rest.find('\n') {
      Some(pos) => &rest[..pos],
      None => rest,
    };
    let name = cut_at_next_label(line);
    if name.chars().count() > 255 {
      continue;
    }
    if name.is_empty() || buyer.as_deref() == Some(name.as_str()) {
      continue;
    }
    return Ok(Some(name));
  }

  // Pattern 2: Railway e-tickets ── look for 中国铁路 with company suffix
  if text.contains("铁路电子客票") || (text.contains("中国铁路") && text.contains("买票请到"))
  {
    let re = Regex::new(r"中国铁路(?:[\w（）]+)?(?:股份|集团)?有限公司")?;
    if let Some(m) = re.find(text) {
      return Ok(Some(m.as_str().to_string()));
    }
    // Fallback to just 中国铁路
    return Ok(Some("中国铁路".to_string()));
  }

  // Pattern 3: bare line containing a company keyword (Walmart, Metro, restaurant, e-commerce).
  for line in text.lines() {
    let line = line.trim();
    if line.chars().count() > 255 {
      continue;
    }
    if ["有限公司", "股份公司", "集团公司"]
      .iter()
      .any(|kw| line.contains(kw))
      && buyer.as_deref() != Some(line)
    {
      return Ok(Some(line.to_string()));
    }
  }
  Ok(None)
}

/// Extract product category and name from fapiao text.
/// Returns (tax_category, product_name) pairs, e.g. ("餐饮服务", "餐饮服务").
fn extract_products(text: &str) -> Result<Vec<(String, String)>> {
  // Pattern: *category*description
  // Category is typically short (2-20 chars), description can be long
  let re = Regex::new(r"\*([^*\n]{2,20})\*([^*\n]{1,200})")?;
  let mut products = vec![];
  for caps in re.captures_iter(text) {
    let (Some(cat), Some(desc)) = (caps.get(1), caps.get(2)) else {
      continue;
    };
    let (cat, desc) = (cat.as_str().trim(), desc.as_str().trim());
    if !cat.is_empty() && !desc.is_empty() {
      products.push((cat.to_string(), desc.to_string()));
    }
  }
  Ok(products)
}

fn clean(str: &str) -> String {
  str.replace(",", "").trim().to_string()
}

fn approx_eq(a: f32, b: f32, tol: Option<f32>) -> bool {
  let tol_num = tol.unwrap_or(0.02);
  (a - b).abs() <= tol_num
}

#[cfg(test)]
mod tests {
  use simple_datetime_rs::Format;

  use super::*;
  use crate::fixtures::{full_fapiao, many};

  // ── helpers ────────────────────────────────────────────────────────────────

  fn parse(text: &str) -> Result<Fapiao> {
    parse_fapiao(text.to_string())
  }

  #[test]
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
    let bytes = include_bytes!("../fixtures/sample.pdf");
    let out = extract(vec![bytes.to_vec()])?;
    println!("{:#?}", out);
    Ok(())
  }

  use std::fs::File;
  use std::io::prelude::*;

  #[test]
  fn test_with_real_fapiao() -> Result<()> {
    let mut f = File::open("fixtures/sample.pdf")?;
    let mut buffer = Vec::new();
    f.read_to_end(&mut buffer)?;
    let fapiaos = extract(vec![buffer])?;
    assert_eq!(fapiaos.len(), 35);
    assert_eq!(
      fapiaos.iter().filter(|f| f.skip).collect::<Vec<_>>().len(),
      6
    );
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
    assert_eq!(result, Some(188.50));
    Ok(())
  }

  #[test]
  fn test_s1_fullwidth_yen() -> Result<()> {
    let result = s1_labeled("年\n2024年1月1日\n（小写）￥99.00")?;
    assert_eq!(result, Some(99.00));
    Ok(())
  }

  #[test]
  fn test_s1_comma_in_amount() -> Result<()> {
    let result = s1_labeled("年\n2024年1月1日\n（小写）¥1,234.56")?;
    assert_eq!(result, Some(1234.56));
    Ok(())
  }

  #[test]
  fn test_s2_didi_style() -> Result<()> {
    // S2: （小写）\nxxx\n¥
    let result = s2_didi("年\n2024年1月1日\n（小写）\n45.60\n¥")?;
    assert_eq!(result, Some(45.60));
    Ok(())
  }

  #[test]
  fn test_s3_meituan_prefix() -> Result<()> {
    // S3: ¥xxx\n大写 — amount precedes 大写
    let result = s3_daxie_prefix("年\n2024年1月1日\n¥55.00\n壹拾贰圆整")?;
    assert_eq!(result, Some(55.00));
    Ok(())
  }

  #[test]
  fn test_s4_ecommerce_amount_follows_daxie() -> Result<()> {
    // S4: 大写\n¥xxx
    let result = s4_daxie_suffix("年\n2024年1月1日\n壹佰贰拾叁圆整\n¥123.00")?;
    assert_eq!(result, Some(123.00));
    Ok(())
  }

  #[test]
  fn test_s4b_restaurant_t_p_v_triplet() -> Result<()> {
    // S4b: 大写\nT\nP\nV where T = P + V
    let result = s4b_restaurant("年\n2024年1月1日\n壹佰圆整\n100.00\n94.34\n5.66\n")?;
    assert_eq!(result, Some(100.00));
    Ok(())
  }

  #[test]
  fn test_s4c_daxie_inline() -> Result<()> {
    let result = parse(
      "电子发票（普通发票）发票号码： 25327000001745891979\n开票日期： 2025年12月30日\n\n购 名称：asdf 销 名称：asdf\n买 售\n方 方\n信123456S01038015统一社会信用代码/纳税人识别号： 信91320691MA1MA9TQ5J统一社会信用代码/纳税人识别号：\n息 息\n项目名称\n规格型号\n单 位\n数 量\n单 价\n金 额税率/征收率\n税 额\n*旅游服务*代订住宿费1 511.3207547169811 511.32 6% 30.68\n\n\n¥511.32 ¥30.68\n合 计\n伍佰肆拾贰圆整 ¥542.00\n价税合计（大写）\n（小写）\n\n备\n注\n\n\n开票人：林哲宇\n\n\n下载次数：1",
    )?;
    assert_eq!(result.amount, Some(542.00));
    Ok(())
  }

  #[test]
  fn test_s5_metro_format() -> Result<()> {
    // S5: P\nV\nT + non-numeric line, where T = P + V
    let result = s5_metro("年\n2024年1月1日\n94.34\n5.66\n100.00\n东西\n")?;
    assert_eq!(result, Some(100.00));
    Ok(())
  }

  #[test]
  fn test_s6_dominos_bare_yen_triplet() -> Result<()> {
    // S6: T\n¥  P\n¥  V\n¥ scattered in text
    let result = parse("年\n2024年1月1日\n100.00\n¥\n94.34\n¥\n5.66\n¥\n")?;
    assert_eq!(result.amount, Some(100.00));
    Ok(())
  }

  #[test]
  fn test_amount_none_when_not_found() -> Result<()> {
    let result = parse("年\n2024年1月1日\n名称：测试公司有限公司")?;
    assert!(result.amount.is_none());
    Ok(())
  }

  #[test]
  fn test_s1_didi_spaced_label() -> Result<()> {
    let result = parse(
      "旅客运输服务\n电子发票（普通发票）发票号码: 25427000000579417592\n开票日期: 2025年12月21日\n\n\n购\n销\n名称：asdf\n名称：滴滴出行科技有限公司武汉分公司\n买\n售\n方\n方\n信\n信\n统一社会信用代码/纳税人识别号：123456S01038015统一社会信用代码/纳税人识别号：91420100MA4KUWB2XE\n息\n息\n项目名称 单\u{a0}\u{a0}价 数\u{a0}\u{a0}量 金\u{a0}\u{a0}额 税率/征收率 税\u{a0}\u{a0}额\n*运输服务*客运服务费1352.14 1 1352.14 3% 40.56\n*运输服务*客运服务费 -52.43 3% -1.57\n\n\n合 计 ¥1299.71 ¥38.99\n出行人 有效身份证件号 出行日期 出发地 到达地 等级 交通工具类型\n\n\n价 税 合 计 （ 大 写 ） 壹仟叁佰叁拾捌圆柒角整\n（ 小 写 ） ¥1338.70\n\n备\n注\n\n\n开票人： 王耸耸\n\ndidi",
    )?;
    assert_eq!(result.amount, Some(1338.70));
    Ok(())
  }

  #[test]
  fn test_s4d_metro_daxie_bare_total() -> Result<()> {
    let result = parse(
      "25427000000380731041\n税\n2025年12月30日\n\n\n武汉总领事馆\n上海麦德龙商贸有限公司武汉古田分公司\n\n\n*熟肉制品*秋林里道斯哈1包1 57.08 57.08 13% 7.42\n尔滨红肠500g\n*蔬菜加工品*麦臻选冷冻1袋1 22.5 22.50免税 ***\n毛豆仁1kg\n*熟肉制品*麦臻选黑猪肉1盒1 40.62 40.62 13% 5.28\n烤肠480g\n*果类加工品*丘比草莓酱1瓶1 21.06 21.06 13% 2.74\n340g\n*谷物*籽籽有味种籽组合1袋1 27.43 27.43 9% 2.47\n800g\n*日用杂品*包装费1 1 0.88 0.88 13% 0.12\n\n169.57 18.03\n壹佰捌拾柒圆陆角整187.60\n719_956891066338_20251230;\n\n\n李丽华",
    )?;
    assert_eq!(result.amount, Some(187.60));
    Ok(())
  }

  #[test]
  fn test_s1b_mixed_label_wagas() -> Result<()> {
    let result = parse(
      "电子发票（普通发票）发票号码： 25427000000001315475\n开票日期： 2025年11月12日\n\n购 销\n    名称：武汉总领事馆 名称：武汉沃歌斯餐饮有限公司\n买 售\n方 方\n信123456S01038015统一社会信用代码/纳税人识别号： 信91420104MA4F20818Y统一社会信用代码/纳税人识别号：\n息 息\n项目名称 规格型号\n单 位 数 量\n单 价 金 额 税率/征收率 税 额\n1 177.358490566037716 177.36 10.646%*餐饮服务*餐饮服务\n\n\n合 计 ¥177.36 ¥10.64\n\n壹佰捌拾捌圆整\n（小写）价税合计（大写） ¥188.00\n\n\n备 注\n\n\n开票人：顾思遥",
    )?;
    // 价税合计 total = 177.36 (pre-tax) + 10.64 (VAT) = 188.00
    assert_eq!(result.amount, Some(188.00));
    Ok(())
  }

  #[test]
  fn test_amount_heji_line_before_daxie_inline_total() -> Result<()> {
    // The 合计 line (¥P ¥V) directly precedes the inline 大写 ¥T total, so
    // s3_daxie_prefix must not mistake the VAT for the total.
    let result = parse(
      "电子发票（普通发票）发票号码： 25422000000209459092\n开票日期： 2025年11月12日\n\n    \n\n购 销\n    名称：武汉总领事馆 名称：武汉卡斯餐饮管理有限公司\n买 售\n方 方\n信123456S01038015统一社会信用代码/纳税人识别号： 信91420105MABXUPAC0J统一社会信用代码/纳税人识别号：\n息 息\n项目名称 规格型号\n单 位\n数 量\n单 价\n金 额 税率/征收率 税 额\n*餐饮服务*餐饮费1%103.54 1.04\n\n\n合 计 ¥103.54 ¥1.04\n壹佰零肆圆伍角捌分 ¥104.58\n价税合计（大写） （小写）\n\n备\n注\n\n\n开票人：郑锦煌\n\n\n下载次数： 1",
    )?;
    assert_eq!(result.amount, Some(104.58));
    Ok(())
  }
  #[test]
  fn test_amount_scattered_xiaoxie_with_heji_line() -> Result<()> {
    // The 小写 total (¥128.68) is scattered away from the 价税合计 block, so
    // it is recovered via 合 计 ¥P ¥V where T = P + V.
    let result = parse(
      "电子发票（普通发票）发票号码： 25322000000563714767\n开票日期： 2025年11月27日\n\n    \n\n购 销\n    名称：武汉总领事馆 名称：江苏鱼跃电子科技有限公司\n买 售\n方 方\n信123456S01038015统一社会信用代码/纳税人识别号： 信91321181748726127F统一社会信用代码/纳税人识别号：\n息 息\n¥128.68\n\n\n名称： 江苏鱼跃电子科技有限公司\n\n统一社会信用代码/纳税人识别号： 91321181748726127F\n\n项目名称 规格型号\n单 位\n数 量\n单 价\n金 额税率/征收率\n税 额\n*医疗仪器器械*【优惠 YE660E新1 113.8761061946903 13%113.88 14.80\n价】鱼跃电子血压计测量\n仪高精准测压仪家用正品\n医用官方旗\n\n\n合 计 ¥113.88 ¥14.80\n壹佰贰拾捌圆陆角捌分\n价税合计（大写） （小写）\nALI722151398022365184\n备\n注\n\n\n开票人：汤晨露\n\n\n下载次数： 1",
    )?;
    assert_eq!(result.amount, Some(128.68));
    Ok(())
  }
  // Test Template for broken parsing
  // #[test]
  // fn test_amount_not_working() -> Result<()> {
  //   let result = parse(
  //   <broken string>
  //   )?;
  //   assert_eq!(result.amount, Some(<actual amount>));
  //   Ok(())
  // }

  // ── VAT strategies ─────────────────────────────────────────────────────────

  #[test]
  fn test_v1_walmart_he_ji() -> Result<()> {
    // V1: 合   计 ¥P ¥V
    let result = parse("年\n2024年1月1日\n（小写）¥188.50\n合     计  ¥176.17  ¥12.33")?;
    assert_eq!(result.vat_amount, Some(12.33));
    Ok(())
  }

  #[test]
  fn test_v1b_heji_suffix() -> Result<()> {
    let result = parse(
      "电子发票（普通发票）发票号码： 25327000001745891979\n开票日期： 2025年12月30日\n\n购 名称：asdf 销 名称：asdf\n买 售\n方 方\n信123456S01038015统一社会信用代码/纳税人识别号： 信91320691MA1MA9TQ5J统一社会信用代码/纳税人识别号：\n息 息\n项目名称\n规格型号\n单 位\n数 量\n单 价\n金 额税率/征收率\n税 额\n*旅游服务*代订住宿费1 511.3207547169811 511.32 6% 30.68\n\n\n¥511.32 ¥30.68\n合 计\n伍佰肆拾贰圆整 ¥542.00\n价税合计（大写）\n（小写）\n\n备\n注\n\n\n开票人：林哲宇\n\n\n下载次数：1",
    )?;
    assert_eq!(result.vat_amount, Some(30.68));
    Ok(())
  }

  #[test]
  fn test_v2_didi_he_ji() -> Result<()> {
    // V2: 合\n计\nP\n¥\nV\n¥
    let result = parse("年\n2024年1月1日\n（小写）\n45.60\n¥\n合\n计\n42.62\n¥\n2.98\n¥")?;
    assert_eq!(result.vat_amount, Some(2.98));
    Ok(())
  }

  #[test]
  fn test_v4_restaurant_vat() -> Result<()> {
    // V4: 大写\nT\nP\nV (third = VAT)
    let result = parse("年\n2024年1月1日\n壹佰圆整\n100.00\n94.34\n5.66\n")?;
    assert_eq!(result.vat_amount, Some(5.66));
    Ok(())
  }

  #[test]
  fn test_v5_metro_vat() -> Result<()> {
    // V5: P\nV\nT + non-numeric line (second = VAT)
    let result = parse("年\n2024年1月1日\n94.34\n5.66\n100.00\n啦啦啦\n")?;
    assert_eq!(result.vat_amount, Some(5.66));
    Ok(())
  }

  #[test]
  fn test_vat_none_when_not_found() -> Result<()> {
    let result = parse("年\n2024年1月1日\n（小写）¥50.00")?;
    assert!(result.vat_amount.is_none());
    Ok(())
  }

  // Template for broken examples
  #[test]
  fn test_v5b_metro_inline() -> Result<()> {
    let result = parse(
      "25427000000380731041\n税\n2025年12月30日\n\n\n武汉总领事馆\n上海麦德龙商贸有限公司武汉古田分公司\n\n\n*熟肉制品*秋林里道斯哈1包1 57.08 57.08 13% 7.42\n尔滨红肠500g\n*蔬菜加工品*麦臻选冷冻1袋1 22.5 22.50免税 ***\n毛豆仁1kg\n*熟肉制品*麦臻选黑猪肉1盒1 40.62 40.62 13% 5.28\n烤肠480g\n*果类加工品*丘比草莓酱1瓶1 21.06 21.06 13% 2.74\n340g\n*谷物*籽籽有味种籽组合1袋1 27.43 27.43 9% 2.47\n800g\n*日用杂品*包装费1 1 0.88 0.88 13% 0.12\n\n169.57 18.03\n壹佰捌拾柒圆陆角整187.60\n719_956891066338_20251230;\n\n\n李丽华",
    )?;
    assert_eq!(result.vat_amount, Some(18.03));
    Ok(())
  }

  // Template for broken examples
  // #[test]
  // fn test_vat_not_working() -> Result<()> {
  //   let result = parse(
  //     <example string>
  //   )?;
  //   assert_eq!(result.vat_amount, Some(<actual amount>));
  //   Ok(())
  // }
  // ── full parse integration ─────────────────────────────────────────────────

  #[test]
  fn test_full_parse_walmart_style() -> Result<()> {
    let result = parse(
      "发票号码：012345678901234\n2024年3月15日\n（小写）¥188.50\n合     计  ¥176.17  ¥12.33\n名称：沃尔玛（湖北）商业零售有限公司\n年\n",
    )?;
    assert!(!result.skip);
    assert_eq!(result.fapiao_number.as_deref(), Some("012345678901234"));
    assert_eq!(date_str(&result).as_deref(), Some("2024-03-15"));
    assert_eq!(result.amount, Some(188.50));
    assert_eq!(result.vat_amount, Some(12.33));
    assert_eq!(
      result.seller.as_deref(),
      Some("沃尔玛(湖北)商业零售有限公司")
    );
    Ok(())
  }

  #[test]
  fn test_full_parse_metro_style() -> Result<()> {
    let result = parse(
      "发票号码：012345678901235\n2024年6月1日\n94.34\n5.66\n100.00\n啦啦啦\n上海麦德龙商贸有限公司武汉分公司\n年\n",
    )?;
    assert!(!result.skip);
    assert_eq!(result.amount, Some(100.00));
    assert_eq!(result.vat_amount, Some(5.66));
    assert!(
      result
        .seller
        .as_deref()
        .is_some_and(|s| s.contains("麦德龙"))
    );
    Ok(())
  }

  // ── seller / products ────────────────────────────────────────────────────

  #[test]
  fn test_seller_skips_buyer_names() -> Result<()> {
    // The buyer is identified via the 购买方 section marker, not a hardcoded list.
    let result = parse(
      "年\n2024年1月1日\n购\n买\n方\n信\n息\n名称：LoremIpsum\n销\n售\n方\n信\n息\n名称：沃尔玛(湖北)商业零售有限公司\n（小写）¥50.00",
    )?;
    assert_eq!(
      result.seller.as_deref(),
      Some("沃尔玛(湖北)商业零售有限公司")
    );
    Ok(())
  }

  #[test]
  fn test_seller_skips_corporate_buyer() -> Result<()> {
    // A corporate buyer (with 有限公司 in its name) is still excluded structurally.
    let result = parse(
      "年\n2024年1月1日\n购\n买\n方\n信\n息\n名称：某某采购有限公司\n销\n售\n方\n信\n息\n名称：沃尔玛(湖北)商业零售有限公司\n（小写）¥50.00",
    )?;
    assert_eq!(
      result.seller.as_deref(),
      Some("沃尔玛(湖北)商业零售有限公司")
    );
    Ok(())
  }

  #[test]
  fn test_seller_didi_names_row_inside_vertical_labels() -> Result<()> {
    // DiDi layout: the names row (buyer then seller) sits between the 购/销
    // header row and the remaining vertical label characters.
    let result = parse(
      "旅客运输服务\n电子发票（普通发票）发票号码: 25317000003403576054\n开票日期: 2025年12月30日\n\n\n购\n销\n名称：我的公司\n名称：上海滴滴畅行科技有限公司\n买\n售\n方\n方\n信\n信\n统一社会信用代码/纳税人识别号：34838410948754统一社会信用代码/纳税人识别号：91310114MA1GW61J6U\n息\n息\n项目名称 单\u{a0}\u{a0}价 数\u{a0}\u{a0}量 金\u{a0}\u{a0}额 税率/征收率 税\u{a0}\u{a0}额\n*运输服务*客运服务费177.28 1 177.28 3% 5.32\n*运输服务*客运服务费 -16.99 3% -0.51\n\n\n合 计 ¥160.29 ¥4.81\n出行人 有效身份证件号 出行日期 出发地 到达地 等级 交通工具类型\n\n\n价 税 合 计 （ 大 写 ） 壹佰陆拾伍圆壹角整\n（ 小 写 ） ¥165.10\n\n备\n注\n\n\n开票人： 于秋红\n\ndidi",
    )?;
    assert_eq!(result.seller.as_deref(), Some("上海滴滴畅行科技有限公司"));
    Ok(())
  }

  #[test]
  fn test_seller_labels_and_names_inline_on_one_line() -> Result<()> {
    // Fully inline layout: 购 名称：<buyer> 销 名称：<seller> on one line,
    // with the remaining label characters on following lines.
    let result = parse(
      "电子发票（普通发票）发票号码： 25327000001745891979\n开票日期： 2025年12月30日\n\n购 名称：武汉总领事馆 销 名称：上海赫程国际旅行社有限公司南通分公司\n买 售\n方 方\n信123456S01038015统一社会信用代码/纳税人识别号： 信91320691MA1MA9TQ5J统一社会信用代码/纳税人识别号：\n息 息\n项目名称\n规格型号\n单 位\n数 量\n单 价\n金 额税率/征收率\n税 额\n*旅游服务*代订住宿费1 511.3207547169811 511.32 6% 30.68\n\n\n¥511.32 ¥30.68\n合 计\n伍佰肆拾贰圆整 ¥542.00\n价税合计（大写）\n（小写）\n\n备\n注\n\n\n开票人：林哲宇\n\n\n下载次数：1",
    )?;
    assert_eq!(
      result.seller.as_deref(),
      Some("上海赫程国际旅行社有限公司南通分公司")
    );
    Ok(())
  }

  #[test]
  fn test_seller_didi_label_order() -> Result<()> {
    // DiDi layout: 销售方 label, 购买方 label, then buyer name, then seller name.
    let result = parse(
      "年\n2024年1月1日\n销\n售\n方\n信\n息\n购\n买\n方\n信\n息\n名称：啦啦啦\n统一社会信用代码/纳税人识别号：123456S01038015\n名称：滴滴出行科技有限公司武汉分公司\n（小写）¥50.00",
    )?;
    assert_eq!(
      result.seller.as_deref(),
      Some("滴滴出行科技有限公司武汉分公司")
    );
    Ok(())
  }

  #[test]
  fn test_seller_buyer_and_seller_names_share_one_line() -> Result<()> {
    let result = parse(
      "电子发票（普通发票）发票号码： 25332000000543115994\n开票日期： 2025年11月27日\n\n    \n\n购 销\n    名称：asdf asd 名称：杭州芙茂电子商务有限公司\n买 售\n方 方\n信123456S01038015统一社会信用代码/纳税人识别号： 信91441900MADWQ9GLXR统一社会信用代码/纳税人识别号：\n息 息\n项目名称 规格型号\n单 位\n数 量\n单 价\n金 额 税率/征收率 税 额\n*家具*家具\n件1 648.6725663716815 648.67 13% 84.33\n\n\n合 计 ¥648.67 ¥84.33\n柒佰叁拾叁圆整 ¥733.00\n价税合计（大写） （小写）\n\n备\n注\n\n\n开票人：李洋\n\n\n下载次数： 1",
    )?;
    assert_eq!(result.seller, Some("杭州芙茂电子商务有限公司".to_string()));
    Ok(())
  }

  #[test]
  fn test_products_extracted() -> Result<()> {
    let result =
      parse("年\n2024年1月1日\n（小写）¥50.00\n*餐饮服务*餐饮服务\n*医疗仪器器械*血压计YE660E\n")?;
    assert_eq!(
      result.products,
      Some(vec![
        ("餐饮服务".to_string(), "餐饮服务".to_string()),
        ("医疗仪器器械".to_string(), "血压计YE660E".to_string()),
      ])
    );
    Ok(())
  }

  #[test]
  fn test_products_empty_when_absent() -> Result<()> {
    let result = parse("年\n2024年1月1日\n（小写）¥50.00")?;
    assert_eq!(result.products, Some(vec![]));
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
    assert_eq!(result.amount, Some(134.00));
    assert_eq!(result.seller.as_deref(), Some("中国铁路"));
    Ok(())
  }

  #[test]
  fn test_railway_ticket_vat_calculation() -> Result<()> {
    // Railway ticket VAT is calculated at 3%: 103 * 3 / 103 = 3.0
    let result = parse(
      "发票号码：12345678901234567890\n2026年06月22日\n电子发票（铁路电子客票）\n票价：¥103.00\n买票请到12306\n中国铁路祝您旅途愉快\n",
    )?;
    assert!(!result.skip);
    assert_eq!(result.vat_amount, Some(3.00));
    Ok(())
  }

  #[test]
  fn test_railway_ticket_vat_calculation_rounding() -> Result<()> {
    // VAT = 134 * 3 / 103 = 3.9029... ≈ 3.90
    let result = parse(
      "2026年06月22日\n电子发票（铁路电子客票）\n票价：¥134.00\n买票请到12306\n中国铁路祝您旅途愉快\n",
    )?;
    assert_eq!(result.vat_amount, Some(3.90));
    Ok(())
  }

  #[test]
  fn test_regular_fapiao_not_affected_by_railway_logic() -> Result<()> {
    // Regular fapiaos use extracted VAT, not the railway calculation.
    let result = parse(
      "发票号码：012345678901234\n2024年3月15日\n（小写）¥188.50\n合     计  ¥176.17  ¥12.33\n名称：沃尔玛（湖北）商业零售有限公司\n年\n",
    )?;
    assert!(!result.skip);
    assert_eq!(result.vat_amount, Some(12.33));
    Ok(())
  }

  #[test]
  fn test_fapiaos_sorted_by_date() -> Result<()> {
    let mut fapiaos = vec![
      Fapiao {
        date: Some(Date::new(2024, 3, 15)),
        ..full_fapiao(0)
      },
      Fapiao {
        date: Some(Date::new(2024, 3, 15)),
        ..full_fapiao(0)
      },
      Fapiao {
        date: Some(Date::new(2024, 1, 10)),
        ..full_fapiao(1)
      },
      Fapiao {
        date: Some(Date::new(2024, 5, 1)),
        ..full_fapiao(2)
      },
    ];
    sort_fapiaos(&mut fapiaos);
    assert!(fapiaos.windows(2).all(|w| w[0].date <= w[1].date));
    assert_eq!(
      fapiaos.iter().map(|f| f.date).collect::<Vec<_>>(),
      vec![
        Some(Date::new(2024, 1, 10)),
        Some(Date::new(2024, 3, 15)),
        Some(Date::new(2024, 3, 15)),
        Some(Date::new(2024, 5, 1)),
      ]
    );
    Ok(())
  }

  #[test]
  fn test_fapiaos_sorted_secondary_by_amount() -> Result<()> {
    let mut fapiaos = vec![
      Fapiao {
        date: Some(Date::new(2024, 3, 15)),
        amount: Some(10.00),
        ..full_fapiao(0)
      },
      Fapiao {
        date: Some(Date::new(2024, 3, 15)),
        amount: Some(20.00),
        ..full_fapiao(0)
      },
    ];
    sort_fapiaos(&mut fapiaos);
    assert_eq!(
      fapiaos.iter().map(|f| f.amount).collect::<Vec<_>>(),
      vec![Some(20.00), Some(10.00)]
    );
    Ok(())
  }

  #[test]
  fn test_fapiaos_sorted_with_none_dates() -> Result<()> {
    let mut fapiaos = vec![
      Fapiao {
        date: Some(Date::new(2024, 3, 15)),
        ..full_fapiao(0)
      },
      Fapiao {
        date: None,
        ..full_fapiao(1)
      },
      Fapiao {
        date: Some(Date::new(2024, 1, 10)),
        ..full_fapiao(2)
      },
    ];
    sort_fapiaos(&mut fapiaos);
    assert!(fapiaos.windows(2).all(|w| w[0].date <= w[1].date));
    Ok(())
  }
}
