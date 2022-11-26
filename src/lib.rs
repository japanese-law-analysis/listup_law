use anyhow::Result;
use csv::ReaderBuilder;
use encoding_rs::{Encoding, SHIFT_JIS};
use log::*;
use quick_xml::{encoding, events::*, Reader};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::{fs, fs::File, io::{BufReader, AsyncReadExt}};
use tokio_stream::StreamExt;

/// 元号
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Era {
  Meiji,
  Taisho,
  Showa,
  Heisei,
  Reiwa,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Date {
  pub era: Era,
  pub year: u16,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub month: Option<u8>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub day: Option<u8>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LawData {
  pub date: Date,
  pub file: String,
  pub name: String,
  pub num: String,
  pub id: String,
}

pub async fn make_law_data(
  reader: &mut Reader<BufReader<File>>,
  file: &str,
  law_id_map: &HashMap<String, LawId>,
) -> Result<Option<LawData>> {
  let utf8 = Encoding::for_label(b"utf-8").unwrap();

  let mut buf = Vec::new();

  let mut is_law_num_mode = false;
  let law_num = Arc::new(Mutex::new(String::new()));
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
            .unwrap()
            .parse()
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
            .map(|s| s.parse().unwrap());
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
            .map(|s| s.parse().unwrap());
          let mut law_date = law_date.lock().unwrap();
          *law_date = Some(Date {
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
        if let b"LawNum" = tag.name().as_ref() {
          is_law_num_mode = false
        }
      }
      Ok(Event::Text(text)) => {
        if is_law_num_mode {
          let mut law_num = law_num.lock().unwrap();
          *law_num = encoding::decode(&text.into_inner(), utf8)?.to_string();
        }
      }
      Ok(Event::Eof) => break,
      Err(e) => panic!("法令名APIの結果のXMLの解析中のエラー: {}", e),
      _ => (),
    }
  }

  let law_date = law_date.lock().unwrap();
  let law_date = &*law_date;
  let law_num = law_num.lock().unwrap();
  let law_num = &*law_num;
  if let Some(law_id_name) = law_id_map.get(law_num) {
    Ok(Some(LawData {
      date: law_date.clone().unwrap(),
      file: file.to_string(),
      name: law_id_name.clone().name,
      num: law_num.clone(),
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
pub async fn make_law_id_data(file_path: &str) -> Result<HashMap<String, LawId>> {
  let s = fs::read(file_path).await?;
  let (res, _, _) = SHIFT_JIS.decode(&s);
  let csv_str_utf8 = res.into_owned();
  let mut reader = ReaderBuilder::new().from_reader(csv_str_utf8.as_bytes());

  let db = Arc::new(Mutex::new(HashMap::new()));

  let mut reader_stream = tokio_stream::iter(reader.records());
  while let Some(data) = reader_stream.next().await {
    let data = data?;
    //print!("{:?}", data.get(1));
    let mut db = db.lock().unwrap();
    let law_id = LawId {
      id: data.get(11).unwrap().to_string(),
      name: data.get(2).unwrap().to_string(),
    };
    db.insert(data.get(1).unwrap().to_string(), law_id);
  }

  let db = db.lock().unwrap();
  let db = &*db;
  Ok(db.clone())
}


pub async fn get_law_from_index(index_file_path: &str) -> Result<Vec<LawData>> {
  let mut f = File::open(index_file_path).await?;
  let mut buf = Vec::new();
  f.read_to_end(&mut buf).await?;
  let file_str = std::str::from_utf8(&buf)?;
  let raw_data_lst = serde_json::from_str(&file_str)?;
  Ok(raw_data_lst)
}


