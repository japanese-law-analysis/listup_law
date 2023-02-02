//!
//! 法律のXMLファイルがあるフォルダから、法律の公布年月日やファイル置き場などのデータをリストアップしてJSONファイルにするソフトウェア
//! # install
//!
//! ```sh
//! cargo install --git "https://github.com/japanese-law-analysis/listup_law.git"
//! ```
//!
//! # Use
//!
//! ```sh
//!  listup_law --output output.json --work "path/to/law_xml_directory"
//! ```
//!
//! で起動します。
//!
//! それぞれのオプションの意味は以下の通りです。
//!
//! - `--output`：法律XMLファイル群の情報のリストを出力するJSONファイル名
//! - `--work`：[e-gov法令検索](https://elaws.e-gov.go.jp/)からダウンロードした全ファイルが入っているフォルダへのpath
//!
//! ---
//! [MIT License](https://github.com/japanese-law-analysis/listup_law/blob/master/LICENSE)
//! (c) 2023 Naoki Kaneko (a.k.a. "puripuri2100")
//!

use anyhow::{anyhow, Result};
use encoding_rs::Encoding;
use quick_xml::{encoding, events::Event, Reader};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::sync::{Arc, Mutex};

/// 元号
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Era {
  Meiji,
  Taisho,
  Showa,
  Heisei,
  Reiwa,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct Date {
  pub ad_year: usize,
  pub era: Era,
  pub year: usize,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub month: Option<usize>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub day: Option<usize>,
}

impl PartialOrd for Date {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    if self.ad_year == other.ad_year {
      match (self.month, other.month) {
        (Some(m1), Some(m2)) => {
          let month_ord = m1.cmp(&m2);
          match month_ord {
            Ordering::Equal => match (self.day, other.day) {
              (Some(d1), Some(d2)) => Some(d1.cmp(&d2)),
              _ => None,
            },
            _ => Some(month_ord),
          }
        }
        _ => None,
      }
    } else {
      Some(self.ad_year.cmp(&other.ad_year))
    }
  }
}

impl Ord for Date {
  fn cmp(&self, other: &Self) -> Ordering {
    match self.partial_cmp(other) {
      Some(ord) => ord,
      None => Ordering::Equal,
    }
  }
}

fn era_to_ad(era: &Era, year: usize) -> usize {
  use Era::*;
  match era {
    Meiji => 1867 + year,
    Taisho => 1911 + year,
    Showa => 1925 + year,
    Heisei => 1988 + year,
    Reiwa => 2018 + year,
  }
}

fn ad_to_era(year: usize, month: usize, day: usize) -> (Era, usize) {
  use Era::*;
  let t = year * 10000 + month * 100 + day;
  if (18681023..=19120729).contains(&t) {
    (Meiji, year - 1867)
  } else if (19120730..=19261224).contains(&t) {
    (Taisho, year - 1920)
  } else if (19261225..=19890107).contains(&t) {
    (Showa, year - 1925)
  } else if (19890108..=20190430).contains(&t) {
    (Heisei, year - 1988)
  } else if 20190501 <= t {
    (Reiwa, year - 2018)
  } else {
    unreachable!()
  }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LawData {
  /// 制定年月日
  pub date: Date,
  /// ファイルのpath
  pub file: String,
  /// 法令名
  pub name: String,
  /// 法令番号
  pub num: String,
  /// 法令ID
  /// https://elaws.e-gov.go.jp/file/LawIdNamingConvention.pdf を参照
  pub id: String,
  /// 過去のバージョン情報
  pub patch: Vec<LawPatchInfo>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LawPatchInfo {
  pub dir_name: String,
  pub file_name: String,
  /// 法令ID
  pub id: String,
  /// 改正・成立年月日
  pub patch_date: Date,
  /// 改正した法令の名前
  pub patch_id: String,
}

pub async fn file_path_to_data(dir_name: &str, file_name: &str) -> Result<(String, LawPatchInfo)> {
  let re = Regex::new(
    r"(?P<id>[\dA-Za-z]+)_(?P<ad_year>[\d]{4})(?P<month>[\d]{2})(?P<day>[\d]{2})_(?P<patch_id>[\dA-Za-z]+).xml",
  )
  .unwrap();
  let captures = re
    .captures(file_name)
    .ok_or_else(|| anyhow!("ファイルのpathのparse失敗：{file_name}"))?;
  let id = captures.name("id").unwrap().as_str();
  let patch_id = captures.name("patch_id").unwrap().as_str();
  let ad_year = captures
    .name("ad_year")
    .unwrap()
    .as_str()
    .parse::<usize>()?;
  let month = captures.name("month").unwrap().as_str().parse::<usize>()?;
  let day = captures.name("day").unwrap().as_str().parse::<usize>()?;
  let (era, year) = ad_to_era(ad_year, month, day);
  let date = Date {
    ad_year,
    era,
    year,
    month: Some(month),
    day: Some(day),
  };
  Ok((
    id.to_string(),
    LawPatchInfo {
      dir_name: dir_name.to_string(),
      file_name: file_name.to_string(),
      id: id.to_string(),
      patch_date: date,
      patch_id: patch_id.to_string(),
    },
  ))
}

pub async fn make_law_data(
  xml_buf: &[u8],
  info: &LawPatchInfo,
  version_info: &[LawPatchInfo],
) -> Result<Option<LawData>> {
  let utf8 = Encoding::for_label(b"utf-8").unwrap();

  let mut reader = Reader::from_reader(xml_buf);
  let mut buf = Vec::new();

  let mut is_law_num_mode = false;
  let mut is_law_name_mode = false;
  let mut is_ruby_mode = false;
  let law_num = Arc::new(Mutex::new(String::new()));
  let law_name = Arc::new(Mutex::new(String::new()));
  let law_date = Arc::new(Mutex::new(None));

  reader.trim_text(true);
  loop {
    match reader.read_event_into_async(&mut buf).await {
      Ok(Event::Start(tag)) => match tag.name().as_ref() {
        b"Law" => {
          let era_str = tag
            .attributes()
            .find(|res| encoding::decode(res.as_ref().unwrap().key.0, utf8).unwrap() == "Era")
            .map(|res| {
              encoding::decode(&res.unwrap().value, utf8)
                .unwrap()
                .to_string()
            })
            .unwrap();
          let era = match &*era_str {
            "Meiji" => Era::Meiji,
            "Taisho" => Era::Taisho,
            "Showa" => Era::Showa,
            "Heisei" => Era::Heisei,
            "Reiwa" => Era::Reiwa,
            _ => {
              println!("{}", &era_str);
              unimplemented!()
            }
          };
          let year = tag
            .attributes()
            .find(|res| encoding::decode(res.as_ref().unwrap().key.0, utf8).unwrap() == "Year")
            .map(|res| {
              encoding::decode(&res.unwrap().value, utf8)
                .unwrap()
                .to_string()
            })
            .and_then(|s| s.parse().ok())
            .unwrap();
          let month = tag
            .attributes()
            .find(|res| {
              let s = encoding::decode(res.as_ref().unwrap().key.0, utf8).unwrap();
              s == "Month" || s == "PromulgateMonth"
            })
            .map(|res| {
              encoding::decode(&res.unwrap().value, utf8)
                .unwrap()
                .to_string()
            })
            .and_then(|s| s.parse().ok());
          let day = tag
            .attributes()
            .find(|res| {
              let s = encoding::decode(res.as_ref().unwrap().key.0, utf8).unwrap();
              s == "Day" || s == "PromulgateDay"
            })
            .map(|res| {
              encoding::decode(&res.unwrap().value, utf8)
                .unwrap()
                .to_string()
            })
            .and_then(|s| s.parse().ok());
          let mut law_date = law_date.lock().unwrap();
          *law_date = Some(Date {
            ad_year: era_to_ad(&era, year),
            era,
            year,
            month,
            day,
          })
        }
        b"LawNum" => is_law_num_mode = true,
        b"LawTitle" => is_law_name_mode = true,
        b"Ruby" => is_ruby_mode = true,
        _ => (),
      },
      Ok(Event::End(tag)) => match tag.name().as_ref() {
        b"LawNum" => is_law_num_mode = false,
        b"LawTitle" => is_law_name_mode = false,
        b"Ruby" => is_ruby_mode = false,
        _ => {}
      },
      Ok(Event::Text(text)) => {
        if is_law_num_mode && !is_ruby_mode {
          let mut law_num = law_num.lock().unwrap();
          *law_num = encoding::decode(&text.into_inner(), utf8)?.to_string();
        } else if is_law_name_mode && !is_ruby_mode {
          let mut law_name = law_name.lock().unwrap();
          *law_name = encoding::decode(&text.into_inner(), utf8)?.to_string();
        }
      }
      Ok(Event::Eof) => break,
      Err(e) => panic!("法令名APIの結果のXMLの解析中のエラー: {e}"),
      _ => (),
    }
  }

  let law_date = law_date.lock().unwrap();
  let law_date = &*law_date;
  let law_num = law_num.lock().unwrap();
  let law_num = &*law_num;
  let law_name = law_name.lock().unwrap();
  let law_name = &*law_name;
  Ok(Some(LawData {
    date: law_date.clone().unwrap(),
    file: format!("{}/{}", info.dir_name, info.file_name),
    name: law_name.clone(),
    num: law_num.clone(),
    id: info.id.clone(),
    patch: version_info.to_vec(),
  }))
}
