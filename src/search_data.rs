use anyhow::Result;
use csv::ReaderBuilder;
use encoding_rs;
use log::*;
use quick_xml::{events::*, Reader};
use serde::Serialize;
use std::collections::HashMap;
use std::{fs, fs::File, io::BufReader, str};

/// 元号
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Era {
  Meiji,
  Taisho,
  Showa,
  Heisei,
  Reiwa,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Date {
  pub era: Era,
  pub year: u16,
  pub month: Option<u8>,
  pub day: Option<u8>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct LawData {
  pub date: Date,
  pub file: String,
  pub name: String,
  pub num: String,
  pub id: String,
}

pub fn make_law_data(
  reader: &mut Reader<BufReader<File>>,
  file: &str,
  law_id_map: &HashMap<String, LawId>,
) -> Result<Option<LawData>> {
  let mut buf = Vec::new();
  let mut law_num = String::new();
  let mut is_law_num_mode = false;
  let mut law_date = None;

  reader.trim_text(true);
  loop {
    match reader.read_event(&mut buf) {
      Ok(Event::Start(tag)) => match tag.name() {
        b"Law" => {
          let era_str = tag
            .attributes()
            .find(|res| reader.decode(res.as_ref().unwrap().key).unwrap() == "Era")
            .map(|res| reader.decode(&res.unwrap().value).unwrap().to_string())
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
            .find(|res| reader.decode(res.as_ref().unwrap().key).unwrap() == "Year")
            .map(|res| reader.decode(&res.unwrap().value).unwrap().to_string())
            .unwrap()
            .parse()
            .unwrap();
          let month = tag
            .attributes()
            .find(|res| {
              let s = reader.decode(res.as_ref().unwrap().key).unwrap();
              s == "Month" || s == "PromulgateMonth"
            })
            .map(|res| reader.decode(&res.unwrap().value).unwrap().to_string())
            .map(|s| s.parse().unwrap());
          let day = tag
            .attributes()
            .find(|res| {
              let s = reader.decode(res.as_ref().unwrap().key).unwrap();
              s == "Day" || s == "PromulgateDay"
            })
            .map(|res| reader.decode(&res.unwrap().value).unwrap().to_string())
            .map(|s| s.parse().unwrap());
          law_date = Some(Date {
            era,
            year,
            month,
            day,
          })
        }
        b"LawNum" => is_law_num_mode = true,
        _ => (),
      },
      Ok(Event::End(tag)) => {
        if let b"LawNum" = tag.name() {
          is_law_num_mode = false
        }
      }
      Ok(Event::Text(text)) => {
        if is_law_num_mode {
          law_num = str::from_utf8(text.escaped())?.to_string();
        }
      }
      Ok(Event::Eof) => break,
      Err(e) => panic!("法令名APIの結果のXMLの解析中のエラー: {}", e),
      _ => (),
    }
  }

  if let Some(law_id_name) = law_id_map.get(&law_num) {
    Ok(Some(LawData {
      date: law_date.unwrap(),
      file: file.to_string(),
      name: law_id_name.clone().name,
      num: law_num,
      id: law_id_name.clone().id,
    }))
  } else {
    error!("Not Found LawId: {}", &law_num);
    Ok(None)
  }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct LawId {
  /// LawIdタグ
  pub id: String,
  /// LawNameタグ 法令名称
  pub name: String,
}

/// `all_law_list.csv`ファイルをもとに法令IDなどを取得する
/// 法令種別,法令番号,法令名,法令名読み,旧法令名,公布日,改正法令名,改正法令番号,改正法令公布日,施行日,施行日備考,法令ID,本文URL,未施行,所管課確認中
pub fn make_law_id_data(file_path: &str) -> Result<HashMap<String, LawId>> {
  let s = fs::read(file_path)?;
  let (res, _, _) = encoding_rs::SHIFT_JIS.decode(&s);
  let csv_str_utf8 = res.into_owned();
  let mut reader = ReaderBuilder::new().from_reader(csv_str_utf8.as_bytes());

  let mut map = HashMap::new();

  for data in reader.records() {
    let data = data?;
    //print!("{:?}", data.get(1));
    let law_id = LawId {
      id: data.get(11).unwrap().to_string(),
      name: data.get(2).unwrap().to_string(),
    };
    map.insert(data.get(1).unwrap().to_string(), law_id);
  }

  Ok(map)
}
