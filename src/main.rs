use anyhow::Result;
use clap::Parser;
use listup_law::LawPatchInfo;
use std::collections::HashMap;
use std::path::Path;
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
async fn get_law_info_lst(work_dir: &str) -> Result<HashMap<String, Vec<LawPatchInfo>>> {
  let mut info_lst: HashMap<String, Vec<LawPatchInfo>> = HashMap::new();
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
          let (_, law_patch_info) =
            listup_law::file_path_to_data(&dir_string, &file_name_string).await?;
          if let Some(lst) = info_lst.get(&law_patch_info.id) {
            let mut l = lst.clone();
            l.push(law_patch_info.clone());
            info_lst.insert(law_patch_info.id, l);
          } else {
            info_lst.insert(law_patch_info.clone().id, vec![law_patch_info]);
          }
        }
      }
    }
  }
  Ok(info_lst)
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

  info!("[START] get law list");
  let law_info_lst = get_law_info_lst(&args.work).await?;
  info!("[END] get law list");

  let mut output_file = File::create(&args.output).await?;
  info!("[START] write json file");
  output_file.write_all("[".as_bytes()).await?;

  let mut is_head = true;

  let mut law_info_lst_stream = tokio_stream::iter(law_info_lst);

  while let Some((id, lst)) = law_info_lst_stream.next().await {
    let mut lst = lst;
    lst.sort();
    lst.reverse();
    let head_info = &lst[0];
    let file_name = format!("{}/{}", head_info.dir_name, head_info.file_name);
    let file_path = Path::new(&args.work)
      .join(&head_info.dir_name)
      .join(&head_info.file_name);
    info!("[START] work file: {id} ({file_name})");
    let mut f = File::open(file_path).await?;
    let mut xml_buf = Vec::new();
    f.read_to_end(&mut xml_buf).await?;
    if let Some(law_data) = listup_law::make_law_data(&xml_buf, head_info, &lst).await? {
      let law_data_json_str = serde_json::to_string(&law_data)?;
      if is_head {
        output_file.write_all("\n".as_bytes()).await?;
        is_head = false;
      } else {
        output_file.write_all(",\n".as_bytes()).await?;
      }
      output_file.write_all(law_data_json_str.as_bytes()).await?;
    } else {
      info!("[ERROR] not found law data: {id} ({file_name})")
    }
    info!("[END] work file: {id} ({file_name})");
  }
  output_file.write_all("\n]".as_bytes()).await?;
  info!("[END] write json file");
  output_file.flush().await?;

  Ok(())
}
