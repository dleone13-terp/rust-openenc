use gdal::vector::FieldValue;
use serde_json::{Value, json};
use std::path::Path;

/// Convert GDAL field value to JSON value
pub fn field_value_to_json(field_value: &FieldValue) -> Option<Value> {
    match field_value {
        FieldValue::StringValue(s) => Some(Value::String(s.clone())),
        FieldValue::IntegerValue(i) => Some(json!(i)),
        FieldValue::Integer64Value(i) => Some(json!(i)),
        FieldValue::RealValue(f) => Some(json!(f)),
        FieldValue::DateTimeValue(dt) => Some(Value::String(dt.to_string())),
        FieldValue::DateValue(d) => Some(Value::String(d.to_string())),
        FieldValue::IntegerListValue(lst) => Some(json!(lst)),
        FieldValue::Integer64ListValue(lst) => Some(json!(lst)),
        FieldValue::RealListValue(lst) => Some(json!(lst)),
        FieldValue::StringListValue(lst) => Some(json!(lst)),
    }
}

/// Extract ENC cell name from directory or file path
pub fn enc_name_from_path(s57_path: &Path) -> String {
    s57_path
        .file_stem()
        .and_then(|name| name.to_str())
        .map(|name| name.split('.').next().unwrap_or(name).to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
