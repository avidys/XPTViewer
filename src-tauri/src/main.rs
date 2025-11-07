#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};
use tauri::Manager;
use xport::transport::{Dataset as XptDataset, File, Value};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FieldMetadata {
  name: String,
  label: Option<String>,
  #[serde(rename = "type")]
  kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DatasetSummary {
  name: String,
  label: Option<String>,
  observation_count: usize,
  fields: Vec<FieldMetadata>,
  rows: Vec<BTreeMap<String, serde_json::Value>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct XptFilePayload {
  path: String,
  datasets: Vec<DatasetSummary>,
}

#[tauri::command]
fn load_xpt(path: String) -> Result<XptFilePayload, String> {
  match load_xpt_impl(Path::new(&path)) {
    Ok(payload) => Ok(payload),
    Err(error) => Err(error.to_string()),
  }
}

fn load_xpt_impl(path: &Path) -> Result<XptFilePayload> {
  let bytes = std::fs::read(path).with_context(|| format!("Unable to read file: {}", path.display()))?;
  let file = File::read(&bytes).context("Unable to parse SAS XPORT file")?;

  let mut datasets = Vec::new();

  for dataset in file.datasets() {
    datasets.push(convert_dataset(dataset)?);
  }

  Ok(XptFilePayload {
    path: path.display().to_string(),
    datasets,
  })
}

fn convert_dataset(dataset: &XptDataset) -> Result<DatasetSummary> {
  let mut fields = Vec::new();
  for var in dataset.variables() {
    let field = FieldMetadata {
      name: var.name().to_string(),
      label: var.label().filter(|s| !s.is_empty()).map(|s| s.to_string()),
      kind: match var.value_type() {
        xport::transport::ValueType::Character => "Character".to_string(),
        xport::transport::ValueType::Numeric => "Numeric".to_string(),
      },
    };
    fields.push(field);
  }

  let mut rows = Vec::new();
  for (index, observation) in dataset.observations().enumerate() {
    if index >= 100 {
      break;
    }

    let mut row = BTreeMap::new();
    for (variable, value) in dataset.variables().iter().zip(observation.values()) {
      row.insert(variable.name().to_string(), convert_value(value));
    }
    rows.push(row);
  }

  Ok(DatasetSummary {
    name: dataset.name().to_string(),
    label: dataset.label().filter(|s| !s.is_empty()).map(|s| s.to_string()),
    observation_count: dataset.observation_count(),
    fields,
    rows,
  })
}

fn convert_value(value: &Value) -> serde_json::Value {
  match value {
    Value::Character(text) => serde_json::Value::String(text.trim_end().to_string()),
    Value::Numeric(Some(number)) => serde_json::Number::from_f64(*number)
      .map(serde_json::Value::Number)
      .unwrap_or(serde_json::Value::Null),
    Value::Numeric(None) => serde_json::Value::Null,
  }
}

fn main() {
  tauri::Builder::default()
    .setup(|app| {
      #[cfg(debug_assertions)]
      {
        let window = app.get_window("main").ok_or_else(|| anyhow!("Missing main window"))?;
        window.set_title("XPTViewer").ok();
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![load_xpt])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
