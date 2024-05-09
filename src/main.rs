use anyhow::{anyhow, Result};
use clap::Parser;
use jplaw_data_types::{
  self,
  article::text_to_str,
  law::{Date, LawId, LawPatchInfo},
  listup::LawInfo,
};
use jplaw_io::{
  error_log, flush_file_value_lst, gen_file_value_lst, info_log, init_logger, wran_log,
  write_value_lst,
};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use tokio::fs::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::StreamExt;
use tracing::*;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
  /// 法令XMLファイル群が置かれている作業ディレクトリへのpath
  #[clap(short, long, value_parser)]
  work: String,
  /// 解析結果を出力するJSONファイルへのpath
  #[clap(short, long, value_parser)]
  output: String,
}

/// e-govで配布されているファイルは"法令データ一式/foobarbaz/foobarbaz.xml"のような形で配布されていて、、
/// work_dirに"法令データ一式"が入ると想定している
async fn get_law_info_lst(work_dir: &str) -> Result<HashMap<LawId, LawInfo>> {
  let mut info_lst: HashMap<LawId, LawInfo> = HashMap::new();
  let mut work_dir_info = read_dir(work_dir).await?;
  let path_re = Regex::new(
    r"(?P<id>[\dA-Za-z]+)_(?P<ad_year>[\d]{4})(?P<month>[\d]{2})(?P<day>[\d]{2})_(?P<patch_id>[\dA-Za-z]+).xml",
  )?;
  while let Some(dir_entry) = work_dir_info.next_entry().await? {
    if dir_entry.file_type().await?.is_dir() {
      let new_path = Path::new(work_dir).join(dir_entry.file_name());
      let mut new_dir = read_dir(&new_path).await?;
      while let Some(new_entry) = new_dir.next_entry().await? {
        if new_entry.file_type().await?.is_file() {
          let dir_string = dir_entry.file_name().to_str().unwrap().to_string();
          let file_name_osstr = new_entry.file_name();
          let file_name_string = file_name_osstr.to_str().unwrap().to_string();
          let file_path = format!("{dir_string}/{file_name_string}");
          let law = japanese_law_xml_schema::parse_xml_file(&file_path)?;
          let date = Date::new(law.era, law.year, None, None);
          let caps = path_re
            .captures(&file_name_string)
            .ok_or(anyhow!("cannot parse file path"))?;
          let law_id = LawId::from_str(&caps["id"]).unwrap();
          if let Some(d) = info_lst.get(&law_id) {
            let patch_date = Date::gen_from_ad(
              caps["ad_year"].parse::<usize>().unwrap(),
              caps["month"].parse::<usize>().unwrap(),
              caps["day"].parse::<usize>().unwrap(),
            );
            let patch_id = LawId::from_str(&caps["patch_id"]).ok();
            d.clone().patch.push(LawPatchInfo {id: law_id, patch_date, patch_id});
          } else {
            let num = law.law_num;
            let name = if let Some(title) = law.law_body.law_title {
              text_to_str(&title.text)
            } else {
              wran_log("not found title", &file_name_string);
              String::new()
            };
            let patch_date = Date::gen_from_ad(
              caps["ad_year"].parse::<usize>().unwrap(),
              caps["month"].parse::<usize>().unwrap(),
              caps["day"].parse::<usize>().unwrap(),
            );
            let patch_id = LawId::from_str(&caps["patch_id"]).ok();
            info_lst.insert(law_id.clone(), LawInfo {
              date,
              name,
              num,
              id: law_id.clone(),
              patch: vec![LawPatchInfo {id: law_id, patch_date, patch_id}]
            });
          }
        }
      }
    }
  }
  Ok(info_lst)
}

#[tokio::main]
async fn main() -> Result<()> {
  let args = Args::parse();

  init_logger().await?;

  info!("[START] get law list");
  let law_info_lst = get_law_info_lst(&args.work).await?;
  info!("[END] get law list");

  info!("[START] write json file");
  let mut output_file = gen_file_value_lst(&args.output).await?;

  let mut is_head = true;

  let mut law_info_lst_stream = tokio_stream::iter(law_info_lst);

  while let Some((id, lst)) = law_info_lst_stream.next().await {
    let mut lst = lst.patch;
    lst.sort_by(|a, b| a.patch_date.cmp(&b.patch_date));
  }
  flush_file_value_lst(&mut output_file).await?;
  info!("[END] write json file");

  Ok(())
}
