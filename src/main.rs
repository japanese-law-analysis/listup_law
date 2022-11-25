use anyhow::Result;
use clap::Parser;
use log::*;
use quick_xml::Reader;
use simplelog::*;
use std::path::{Path, PathBuf};
use tokio::fs::*;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio_stream::StreamExt;

mod search_data;

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

/// e-govで配布されているファイルは"法令データ一式/foobarbaz/foobarbaz.xml"のような形で配布されていて、、
/// work_dirに"法令データ一式"が入ると想定している
async fn get_law_xml_file_path_lst(work_dir: &str) -> Result<Vec<(String, PathBuf)>> {
  let mut lst = vec![];
  let mut work_dir_info = read_dir(work_dir).await?;
  while let Some(dir_entry) = work_dir_info.next_entry().await? {
    if dir_entry.file_type().await?.is_dir() {
      let new_path = Path::new(work_dir).join(dir_entry.file_name());
      let mut new_dir = read_dir(&new_path).await?;
      while let Some(new_entry) = new_dir.next_entry().await? {
        if new_entry.file_type().await?.is_file() {
          let name = Path::new(&dir_entry.file_name()).join(new_entry.file_name());
          lst.push((name.to_string_lossy().to_string(), new_entry.path()))
        }
      }
    }
  }
  Ok(lst)
}

fn init_logger() -> Result<()> {
  CombinedLogger::init(vec![TermLogger::new(
    LevelFilter::Info,
    Config::default(),
    TerminalMode::Mixed,
    ColorChoice::Auto,
  )])?;
  Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
  let args = Args::parse();

  init_logger()?;

  info!("[START] get law id: {:?}", &args.input);
  let law_id_data = search_data::make_law_id_data(&args.input).await?;
  info!("[END] get law id: {:?}", &args.input);

  info!("[START] get law list");
  let law_xml_file_path_lst = get_law_xml_file_path_lst(&args.work).await?;
  info!("[END] get law list");

  let mut output_file = File::create(&args.output).await?;
  info!("[START] write json file");
  output_file.write_all("[".as_bytes()).await?;

  let mut is_head = true;

  let mut law_xml_file_path_stream = tokio_stream::iter(law_xml_file_path_lst.iter());

  while let Some((file_name, file_path)) = law_xml_file_path_stream.next().await {
    info!("[START] work file: {:?}", file_path);
    let f = File::open(file_path).await?;
    let mut reader = Reader::from_reader(BufReader::new(f));
    info!("[START] data write: {:?}", file_path);
    if let Some(law_data) = search_data::make_law_data(&mut reader, file_name, &law_id_data).await?
    {
      let law_data_json_str = serde_json::to_string(&law_data)?;
      if is_head {
        output_file.write_all("\n".as_bytes()).await?;
        is_head = false;
      } else {
        output_file.write_all(",\n".as_bytes()).await?;
      }
      output_file.write_all(law_data_json_str.as_bytes()).await?;
      info!("[END] data write: {:?}", file_path);
    }
  }
  output_file.write_all("\n]".as_bytes()).await?;
  info!("[END write json file");
  output_file.flush().await?;

  Ok(())
}
