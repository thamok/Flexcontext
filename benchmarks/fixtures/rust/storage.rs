pub struct Record { pub key: String, pub value: String }
/// Encode a record as a tab-separated line.
pub fn encode_record(record: &Record) -> String { format!("{}\t{}\n", record.key, record.value) }
/// Persist all records to a temporary file before atomic rename.
pub fn save_records(records: &[Record], path: &std::path::Path) -> std::io::Result<()> { let text: String = records.iter().map(encode_record).collect(); let temp = path.with_extension("tmp"); std::fs::write(&temp, text)?; std::fs::rename(temp, path) }
/// Decode a tab-separated storage line.
pub fn decode_record(line: &str) -> Option<Record> { let (key, value) = line.trim_end().split_once('\t')?; Some(Record { key: key.into(), value: value.into() }) }
pub fn storage_status_label() -> &'static str { "Storage ready" }
