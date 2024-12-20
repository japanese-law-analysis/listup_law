#![recursion_limit = "256"]
use anyhow::{anyhow, Result};
use clap::Parser;
use jplaw_data_types::{
  self,
  law::{Date, LawId, LawPatchInfo},
  listup::LawInfo,
};
use jplaw_io::{
  end_log, flush_file_value_lst, gen_file_value_lst, info_log, init_logger, start_log, wran_log,
  write_value_lst,
};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use tokio::fs::*;
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
          let file_path = format!("{work_dir}/{dir_string}/{file_name_string}");
          info_log("xml path", &file_path);
          let law = japanese_law_xml_schema::parse_xml_file(&file_path)?;
          let date = Date::new(law.era, law.year, None, None);
          let caps = path_re
            .captures(&file_name_string)
            .ok_or(anyhow!("cannot parse file path"))?;
          let law_id = LawId::from_str(&caps["id"]).unwrap();
          info_log("law_id", &law_id.to_string());
          if let Some(d) = info_lst.get(&law_id) {
            let patch_date = Date::gen_from_ad(
              caps["ad_year"].parse::<usize>().unwrap(),
              caps["month"].parse::<usize>().unwrap(),
              caps["day"].parse::<usize>().unwrap(),
            );
            let re_patch_id_str = &caps["patch_id"];
            let patch_id = LawId::from_str(re_patch_id_str).ok();
            if let Some(id) = &patch_id {
              let s = format!("{id}");
              if re_patch_id_str != s {
                error!("{} != {}({:?})", re_patch_id_str, id, id);
                panic!()
              }
            }
            let mut patch = d.clone().patch;
            patch.push(LawPatchInfo {
              id: law_id.clone(),
              patch_date,
              patch_id,
            });
            info_lst.insert(law_id, LawInfo { patch, ..d.clone() });
          } else {
            let num = law.law_num;
            let name = if let Some(title) = law.law_body.law_title {
              title.text.to_string()
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
            info_lst.insert(
              law_id.clone(),
              LawInfo {
                date,
                name,
                num,
                id: law_id.clone(),
                patch: vec![LawPatchInfo {
                  id: law_id,
                  patch_date,
                  patch_id,
                }],
              },
            );
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

  let mut law_info_lst_stream = tokio_stream::iter(law_info_lst);

  while let Some((id, data)) = law_info_lst_stream.next().await {
    start_log("write law info", &id);
    let mut lst = data.clone().patch;
    lst.sort_by(|a, b| a.patch_date.cmp(&b.patch_date));
    info_log("patch list", &lst);
    write_value_lst(&mut output_file, data).await?;
    end_log("write law info", &id);
  }
  flush_file_value_lst(&mut output_file).await?;
  info!("[END] write json file");

  Ok(())
}
