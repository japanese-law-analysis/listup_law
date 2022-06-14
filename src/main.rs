use anyhow::Result;
use clap::Parser;
use log::*;
use quick_xml::Reader;
use simplelog::*;
use std::fs::*;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

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

fn get_law_xml_file_path_lst(work_dir: &str) -> Result<Vec<(String, PathBuf)>> {
  let mut lst = vec![];
  for entry in read_dir(work_dir)? {
    let entry = entry?;
    if entry.file_type()?.is_dir() {
      let new_dir = Path::new(work_dir).join(entry.file_name());
      for new_entry in read_dir(&new_dir)? {
        let new_entry = new_entry?;
        if new_entry.file_type()?.is_file() {
          let name = Path::new(&entry.file_name()).join(new_entry.file_name());
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

fn main() -> Result<()> {
  let args = Args::parse();

  init_logger()?;

  info!("[START] get law id: {:?}", &args.input);
  let law_id_data = search_data::make_law_id_data(&args.input)?;
  info!("[END] get law id: {:?}", &args.input);

  info!("[START] get law list");
  let law_xml_file_path_lst = get_law_xml_file_path_lst(&args.work)?;
  info!("[END] get law list");

  let mut output_file = File::create(&args.output)?;
  info!("[START] write json file");
  output_file.write_all("[".as_bytes())?;

  let mut is_head = true;

  for (file_name, file_path) in law_xml_file_path_lst.iter() {
    info!("[START] work file: {:?}", file_path);
    let mut reader = Reader::from_reader(BufReader::new(File::open(file_path)?));
    info!("[START] data write: {:?}", file_path);
    if let Some(law_data) = search_data::make_law_data(&mut reader, file_name, &law_id_data)? {
      let law_data_json_str = serde_json::to_string(&law_data)?;
      if is_head {
        output_file.write_all("\n".as_bytes())?;
        is_head = false;
      } else {
        output_file.write_all(",\n".as_bytes())?;
      }
      output_file.write_all(law_data_json_str.as_bytes())?;
      info!("[END] data write: {:?}", file_path);
    }
  }
  output_file.write_all("\n]".as_bytes())?;
  info!("[END write json file");
  output_file.flush()?;

  Ok(())
}
