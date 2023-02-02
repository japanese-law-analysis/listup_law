[![Workflow Status](https://github.com/japanese-law-analysis/listup_law/workflows/Rust%20CI/badge.svg)](https://github.com/japanese-law-analysis/listup_law/actions?query=workflow%3A%22Rust%2BCI%22)

# listup_law


法律のXMLファイルがあるフォルダから、法律の公布年月日やファイル置き場などのデータをリストアップしてJSONファイルにするソフトウェア
## install

```sh
cargo install --git "https://github.com/japanese-law-analysis/listup_law.git"
```

## Use

```sh
 listup_law --output output.json --work "path/to/law_xml_directory"
```

で起動します。

それぞれのオプションの意味は以下の通りです。

- `--output`：法律XMLファイル群の情報のリストを出力するJSONファイル名
- `--work`：[e-gov法令検索](https://elaws.e-gov.go.jp/)からダウンロードした全ファイルが入っているフォルダへのpath

---
[MIT License](https://github.com/japanese-law-analysis/listup_law/blob/master/LICENSE)
(c) 2023 Naoki Kaneko (a.k.a. "puripuri2100")


License: MIT
