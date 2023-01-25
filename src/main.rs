use anyhow::Result;
use clap::Parser;
use quick_xml::Reader;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs::*;
use tokio::io::{AsyncWriteExt, BufReader};
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
  /// 法令IDが書かれたCSVファイルへのpath
  #[clap(short, long, value_parser)]
  input: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LawPathData {
  /// 法律ID
  id: String,
  /// 法律施工年月日
  date: String,
  /// 改正する法律の法律ID
  patch_ver: Option<String>,
  /// ファイルpath
  path: PathBuf,
  /// ファイル名
  file_name: String,
}

/// e-govで配布されているファイルは"法令データ一式/foobarbaz/foobarbaz.xml"のような形で配布されていて、、
/// work_dirに"法令データ一式"が入ると想定している
async fn get_law_xml_file_path_lst(work_dir: &str) -> Result<HashMap<String, LawPathData>> {
  let mut file_path_lst = HashMap::new();
  let mut work_dir_info = read_dir(work_dir).await?;
  while let Some(dir_entry) = work_dir_info.next_entry().await? {
    if dir_entry.file_type().await?.is_dir() {
      let new_path = Path::new(work_dir).join(dir_entry.file_name());
      let mut new_dir = read_dir(&new_path).await?;
      while let Some(new_entry) = new_dir.next_entry().await? {
        if new_entry.file_type().await?.is_file() {
          let dir_string = dir_entry.file_name().to_str().unwrap().to_string();
          let file_name_osstr = new_entry.file_name();
          let file_name_string = file_name_osstr.to_str().unwrap().to_string();
          let file_name_split = file_name_string.split('_').collect::<Vec<_>>();
          let id = file_name_split[0].to_string();
          let date = file_name_split[1].to_string();
          let patch_ver = file_name_split.get(2).map(|s| s.to_string());
          let path = new_path.join(&file_name_osstr);
          let path_data = LawPathData {
            id: id.clone(),
            date: date.clone(),
            patch_ver: patch_ver.clone(),
            path: path.to_path_buf(),
            file_name: format!("{dir_string}/{file_name_string}"),
          };
          let d = file_path_lst.clone();
          let old_date_opt = d.get(&id).map(|d: &LawPathData| &d.date);
          match old_date_opt {
            Some(old_date) => {
              if old_date < &date {
                file_path_lst.insert(id.clone(), path_data);
              }
            }
            None => {
              file_path_lst.insert(id.clone(), path_data);
            }
          }
        }
      }
    }
  }
  Ok(file_path_lst)
}

async fn init_logger() -> Result<()> {
  let subscriber = tracing_subscriber::fmt()
    .with_max_level(tracing::Level::INFO)
    .finish();
  tracing::subscriber::set_global_default(subscriber)?;
  Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
  let args = Args::parse();

  init_logger().await?;

  info!("[START] get law id: {:?}", &args.input);
  let law_id_data = listup_law::make_law_id_data(&args.input).await?;
  info!("[END] get law id: {:?}", &args.input);

  info!("[START] get law list");
  let law_xml_file_path_lst = get_law_xml_file_path_lst(&args.work).await?;
  info!("[END] get law list");

  let mut output_file = File::create(&args.output).await?;
  info!("[START] write json file");
  output_file.write_all("[".as_bytes()).await?;

  let mut is_head = true;

  let mut law_xml_file_path_stream = tokio_stream::iter(law_xml_file_path_lst.iter());

  while let Some((_, law_path_data)) = law_xml_file_path_stream.next().await {
    let file_path = &law_path_data.path;
    let file_name = &law_path_data.file_name;
    info!("[START] work file: {:?}", &file_path);
    let f = File::open(file_path).await?;
    let mut reader = Reader::from_reader(BufReader::new(f));
    info!("[START] data write: {:?}", law_path_data.path);
    if let Some(law_data) = listup_law::make_law_data(&mut reader, file_name, &law_id_data).await? {
      let law_data_json_str = serde_json::to_string(&law_data)?;
      if is_head {
        output_file.write_all("\n".as_bytes()).await?;
        is_head = false;
      } else {
        output_file.write_all(",\n".as_bytes()).await?;
      }
      output_file.write_all(law_data_json_str.as_bytes()).await?;
      info!("[END] data write: {:?}", law_path_data.path);
    }
  }
  output_file.write_all("\n]".as_bytes()).await?;
  info!("[END] write json file");
  output_file.flush().await?;

  Ok(())
}
